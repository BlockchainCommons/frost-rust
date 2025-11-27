use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, XID};
use bc_envelope::prelude::*;
use bc_xid::XIDDocument;
use clap::Parser;
use frost_ed25519 as frost;
use gstp::SealedResponse;
use tokio::runtime::Runtime;

use super::super::common::{OptionalStorageSelector, parse_arid_ur};
use crate::{
    cmd::{
        is_verbose, registry::participants_file_path, storage::StorageClient,
    },
    registry::{PendingRequests, Registry},
};

/// Collect Round 1 responses from all participants (coordinator only).
#[derive(Debug, Parser)]
#[group(skip)]
pub struct CommandArgs {
    #[command(flatten)]
    storage: OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Wait up to this many seconds for each response to appear
    #[arg(long = "timeout", value_name = "SECONDS")]
    timeout: Option<u64>,

    /// Group ID to collect Round 1 responses for
    #[arg(value_name = "GROUP_ID")]
    group_id: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let selection = selection
            .context("Hubert storage is required for round1 collect")?;

        let registry_path = participants_file_path(self.registry.clone())?;
        let mut registry =
            Registry::load(&registry_path).with_context(|| {
                format!(
                    "Failed to load registry at {}",
                    registry_path.display()
                )
            })?;

        let owner = registry
            .owner()
            .context("Registry owner is required")?
            .clone();

        let group_id = parse_arid_ur(&self.group_id)?;
        let group_record = registry
            .group(&group_id)
            .context("Group not found in registry")?
            .clone();

        // Verify we are the coordinator
        if group_record.coordinator().xid() != &owner.xid() {
            bail!(
                "Only the coordinator can collect Round 1 responses. \
                 Coordinator: {}, Owner: {}",
                group_record.coordinator().xid().ur_string(),
                owner.xid().ur_string()
            );
        }

        let pending_requests = group_record.pending_requests();
        if pending_requests.is_empty() {
            bail!(
                "No pending requests for this group. \
                 Round 1 may already be collected."
            );
        }

        if is_verbose() {
            eprintln!(
                "Collecting Round 1 responses from {} participants...",
                pending_requests.len()
            );
        }

        let mut round1_packages: Vec<(XID, frost::keys::dkg::round1::Package)> =
            Vec::new();
        let mut next_response_arids: Vec<(XID, ARID)> = Vec::new();
        let mut errors: Vec<(XID, String)> = Vec::new();

        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        // Collect invite responses from each participant's response ARID
        for (participant_xid, collect_from_arid) in
            pending_requests.iter_collect()
        {
            let participant_name = registry
                .participant(participant_xid)
                .map(|r| {
                    r.pet_name()
                        .map(|s| s.to_owned())
                        .unwrap_or_else(|| participant_xid.ur_string())
                })
                .unwrap_or_else(|| participant_xid.ur_string());

            if is_verbose() {
                eprintln!("{}...", participant_name);
            }

            match fetch_and_validate_response(
                &runtime,
                &client,
                collect_from_arid,
                self.timeout,
                owner.xid_document(),
                &group_id,
            ) {
                Ok((package, next_arid)) => {
                    round1_packages.push((*participant_xid, package));
                    next_response_arids.push((*participant_xid, next_arid));
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    errors.push((*participant_xid, e.to_string()));
                }
            }
        }

        if !errors.is_empty() {
            if is_verbose() {
                eprintln!();
                eprintln!(
                    "Failed to collect from {} participants:",
                    errors.len()
                );
            }
            for (xid, error) in &errors {
                eprintln!("  {}: {}", xid.ur_string(), error);
            }
            bail!(
                "Round 1 collection incomplete: {} of {} responses failed",
                errors.len(),
                pending_requests.len()
            );
        }

        // Persist all round1 packages as an object keyed by participant XID
        let packages_dir = group_state_dir(&registry_path, &group_id);
        fs::create_dir_all(&packages_dir).with_context(|| {
            format!(
                "Failed to create group state directory {}",
                packages_dir.display()
            )
        })?;

        let round1_packages_path = packages_dir.join("collected_round1.json");
        let packages_json: serde_json::Map<String, serde_json::Value> =
            round1_packages
                .iter()
                .map(|(xid, package)| {
                    // Keep packages in their canonical serialized form
                    // (including per-package header) so
                    // they can be fed directly into
                    // FROST DKG part2 without reconstruction.
                    (xid.ur_string(), serde_json::to_value(package).unwrap())
                })
                .collect();
        fs::write(
            &round1_packages_path,
            serde_json::to_vec_pretty(&packages_json)?,
        )
        .with_context(|| {
            format!("Failed to write {}", round1_packages_path.display())
        })?;

        // Update pending_requests with the ARIDs where participants want to
        // receive Round 2 requests (extracted from their invite
        // responses as "response_arid") These become the "send_to"
        // ARIDs for the round2 send phase.
        let mut new_pending = PendingRequests::new();
        for (xid, send_to_arid) in &next_response_arids {
            new_pending.add_send_only(*xid, *send_to_arid);
        }
        let group_record = registry
            .group_mut(&group_id)
            .context("Group not found in registry")?;
        group_record.set_pending_requests(new_pending);
        registry.save(&registry_path)?;

        // Display relative path from current directory if possible
        let display_path = std::env::current_dir()
            .ok()
            .and_then(|cwd| round1_packages_path.strip_prefix(&cwd).ok())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| round1_packages_path.clone());

        if is_verbose() {
            eprintln!();
            eprintln!(
                "Collected {} Round 1 packages. Saved to {}",
                round1_packages.len(),
                display_path.display()
            );
        } else {
            // Still provide path in non-verbose mode
            println!("{}", display_path.display());
        }

        Ok(())
    }
}

fn fetch_and_validate_response(
    runtime: &Runtime,
    client: &StorageClient,
    response_arid: &ARID,
    timeout: Option<u64>,
    coordinator: &XIDDocument,
    expected_group_id: &ARID,
) -> Result<(frost::keys::dkg::round1::Package, ARID)> {
    // Fetch from Hubert
    let envelope = runtime.block_on(async {
        client
            .get(response_arid, timeout)
            .await?
            .context("Response not found in Hubert storage")
    })?;

    let coordinator_private_keys =
        coordinator.inception_private_keys().ok_or_else(|| {
            anyhow::anyhow!(
                "Coordinator XID document has no inception private keys"
            )
        })?;

    // Parse as GSTP SealedResponse
    let now = Date::now();
    let sealed_response = SealedResponse::try_from_encrypted_envelope(
        &envelope,
        None,
        Some(now),
        coordinator_private_keys,
    )?;

    // Check for errors - error() returns Ok(envelope) if error exists
    if let Ok(error) = sealed_response.error() {
        let reason = error
            .object_for_predicate("reason")
            .ok()
            .and_then(|e| e.extract_subject::<String>().ok())
            .unwrap_or_else(|| "unknown reason".to_string());
        bail!("Participant rejected invite: {}", reason);
    }

    // Extract the result envelope
    let result = sealed_response
        .result()
        .context("Response has no result envelope")?;

    // Validate the response structure
    let function: String = result.extract_subject()?;
    if function != "dkgInviteResponse" {
        bail!("Unexpected response function: {}", function);
    }

    let group_id: ARID = result.extract_object_for_predicate("group")?;
    if group_id != *expected_group_id {
        bail!(
            "Response group ID {} does not match expected {}",
            group_id.ur_string(),
            expected_group_id.ur_string()
        );
    }

    // Extract the participant's next response ARID (where they want Round 2)
    let next_response_arid: ARID =
        result.extract_object_for_predicate("response_arid")?;

    // Extract round1_package as an envelope wrapping the byte string
    let round1_envelope: Envelope =
        result.object_for_predicate("round1_package")?;
    let round1_json: bc_components::JSON = round1_envelope
        .extract_subject()
        .context("round1_package missing")?;
    let round1_package: frost::keys::dkg::round1::Package =
        serde_json::from_slice(round1_json.as_bytes())
            .context("Failed to deserialize Round 1 package")?;

    Ok((round1_package, next_response_arid))
}

fn group_state_dir(registry_path: &Path, group_id: &ARID) -> PathBuf {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("group-state").join(group_id.hex())
}
