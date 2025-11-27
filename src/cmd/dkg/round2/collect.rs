use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, XID};
use bc_envelope::prelude::*;
use clap::Parser;
use frost_ed25519 as frost;
use gstp::SealedResponse;
use tokio::runtime::Runtime;

use super::super::common::OptionalStorageSelector;
use crate::{
    cmd::{
        dkg::common::parse_arid_ur, is_verbose,
        registry::participants_file_path, storage::StorageClient,
    },
    registry::{PendingRequests, Registry},
};

/// Collect Round 2 responses from all participants (coordinator only).
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

    /// Group ID to collect Round 2 responses for
    #[arg(value_name = "GROUP_ID")]
    group_id: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let selection = selection
            .context("Hubert storage is required for round2 collect")?;

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

        // Verify coordinator
        if group_record.coordinator().xid() != &owner.xid() {
            bail!(
                "Only the coordinator can collect Round 2 responses. \
                 Coordinator: {}, Owner: {}",
                group_record.coordinator().xid().ur_string(),
                owner.xid().ur_string()
            );
        }

        let pending_requests = group_record.pending_requests();
        if pending_requests.is_empty() {
            bail!(
                "No pending requests for this group. \
                 Did you run 'frost dkg round2 send'?"
            );
        }

        if is_verbose() {
            eprintln!(
                "Collecting Round 2 responses from {} participants...",
                pending_requests.len()
            );
        }

        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        let mut collected: Vec<CollectedRound2> = Vec::new();
        let mut errors: Vec<(XID, String)> = Vec::new();

        for (participant_xid, collect_from_arid) in
            pending_requests.iter_collect()
        {
            let name = registry
                .participant(participant_xid)
                .and_then(|r| r.pet_name().map(|s| s.to_owned()))
                .unwrap_or_else(|| participant_xid.ur_string());

            if is_verbose() {
                eprintln!("{}...", name);
            }

            match fetch_response(
                &runtime,
                &client,
                collect_from_arid,
                self.timeout,
                owner.xid_document(),
                &group_id,
                participant_xid,
            ) {
                Ok(collected_entry) => {
                    collected.push(collected_entry);
                }
                Err(err) => {
                    if is_verbose() {
                        eprintln!("error: {}", err);
                    }
                    errors.push((*participant_xid, err.to_string()));
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
                for (xid, error) in &errors {
                    eprintln!("  {}: {}", xid.ur_string(), error);
                }
            }
            bail!(
                "Round 2 collection incomplete: {} of {} responses failed",
                errors.len(),
                pending_requests.len()
            );
        }

        // Persist collected round2 packages keyed by sender XID
        let state_dir = group_state_dir(&registry_path, &group_id);
        fs::create_dir_all(&state_dir).with_context(|| {
            format!(
                "Failed to create group state directory {}",
                state_dir.display()
            )
        })?;

        let collected_path = state_dir.join("collected_round2.json");
        let mut root = serde_json::Map::new();
        for entry in &collected {
            let mut sender_map = serde_json::Map::new();
            sender_map.insert(
                "response_arid".to_string(),
                serde_json::Value::String(entry.next_response_arid.ur_string()),
            );
            let mut packages_json = serde_json::Map::new();
            for (recipient, package) in &entry.packages {
                // Preserve packages exactly as serialized (including header) so
                // they can be handed directly to part3 without reconstruction.
                packages_json.insert(
                    recipient.ur_string(),
                    serde_json::to_value(package).expect("round2 package JSON"),
                );
            }
            sender_map.insert(
                "packages".to_string(),
                serde_json::Value::Object(packages_json),
            );
            root.insert(
                entry.sender.ur_string(),
                serde_json::Value::Object(sender_map),
            );
        }
        fs::write(&collected_path, serde_json::to_vec_pretty(&root)?)
            .with_context(|| {
                format!("Failed to write {}", collected_path.display())
            })?;

        // Build pending requests for finalize send: where to POST finalize to
        // each participant
        let mut new_pending = PendingRequests::new();
        for entry in &collected {
            new_pending.add_send_only(entry.sender, entry.next_response_arid);
        }

        let group_record = registry
            .group_mut(&group_id)
            .context("Group not found in registry")?;
        group_record.set_pending_requests(new_pending);
        registry.save(&registry_path)?;

        if is_verbose() {
            eprintln!();
            eprintln!(
                "Collected {} Round 2 responses. Saved to {}",
                collected.len(),
                collected_path.display()
            );
        } else {
            println!("{}", collected_path.display());
        }

        Ok(())
    }
}

struct CollectedRound2 {
    sender: XID,
    next_response_arid: ARID,
    packages: Vec<(XID, frost::keys::dkg::round2::Package)>,
}

fn fetch_response(
    runtime: &Runtime,
    client: &StorageClient,
    arid: &ARID,
    timeout: Option<u64>,
    coordinator: &bc_xid::XIDDocument,
    expected_group: &ARID,
    expected_sender: &XID,
) -> Result<CollectedRound2> {
    let envelope = runtime.block_on(async {
        client
            .get(arid, timeout)
            .await?
            .context("Response not found in Hubert storage")
    })?;

    let coordinator_keys =
        coordinator.inception_private_keys().ok_or_else(|| {
            anyhow::anyhow!("Coordinator XID document has no private keys")
        })?;

    let now = Date::now();
    let sealed = SealedResponse::try_from_encrypted_envelope(
        &envelope,
        None,
        Some(now),
        coordinator_keys,
    )?;

    if let Ok(error) = sealed.error() {
        let reason = error
            .object_for_predicate("reason")
            .ok()
            .and_then(|e| e.extract_subject::<String>().ok())
            .unwrap_or_else(|| "unknown reason".to_string());
        bail!("Participant reported error: {}", reason);
    }

    let result = sealed.result().context("Response has no result envelope")?;

    let function: String = result.extract_subject()?;
    if function != "dkgRound2Response" {
        bail!("Unexpected response function: {}", function);
    }

    let group_id: ARID = result.extract_object_for_predicate("group")?;
    if &group_id != expected_group {
        bail!(
            "Response group ID {} does not match expected {}",
            group_id.ur_string(),
            expected_group.ur_string()
        );
    }

    let sender_xid: XID = result.extract_object_for_predicate("participant")?;
    if &sender_xid != expected_sender {
        bail!(
            "Response participant {} does not match expected {}",
            sender_xid.ur_string(),
            expected_sender.ur_string()
        );
    }

    let next_response_arid: ARID =
        result.extract_object_for_predicate("response_arid")?;

    let mut packages = Vec::new();
    for pkg_env in result.objects_for_predicate("round2Package") {
        let recipient: XID =
            pkg_env.extract_object_for_predicate("recipient")?;
        let pkg_json: bc_components::JSON =
            pkg_env.extract_subject().context("round2Package missing")?;
        let pkg: frost::keys::dkg::round2::Package =
            serde_json::from_slice(pkg_json.as_bytes())
                .context("Failed to deserialize round2 package")?;
        packages.push((recipient, pkg));
    }

    Ok(CollectedRound2 { sender: sender_xid, next_response_arid, packages })
}

fn group_state_dir(registry_path: &Path, group_id: &ARID) -> PathBuf {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("group-state").join(group_id.hex())
}
