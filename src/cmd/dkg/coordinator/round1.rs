use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, XID};
use bc_envelope::prelude::*;
use bc_xid::XIDDocument;
use clap::Args;
use frost_ed25519 as frost;
use gstp::{SealedRequest, SealedResponse};
use tokio::runtime::Runtime;

use crate::{
    cmd::{
        dkg::common::{OptionalStorageSelector, parse_arid_ur},
        is_verbose,
        registry::participants_file_path,
        storage::StorageClient,
    },
    registry::{PendingRequests, Registry},
};

/// Collect Round 1 responses and dispatch Round 2 requests (coordinator).
#[derive(Debug, Args)]
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

    /// Preview one of the Round 2 requests while sending
    #[arg(long = "preview")]
    preview: bool,

    /// Group ID to collect Round 1 responses for
    #[arg(value_name = "GROUP_ID")]
    group_id: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let selection =
            selection.context("Hubert storage is required for round1")?;

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
        let owner_doc = owner.xid_document().clone();

        let group_id = parse_arid_ur(&self.group_id)?;
        let group_record = registry
            .group(&group_id)
            .context("Group not found in registry")?
            .clone();

        // Verify we are the coordinator
        if group_record.coordinator().xid() != &owner.xid() {
            bail!(
                "Only the coordinator can collect and send Round 2 requests. \
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

        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        let collection = collect_round1(
            &runtime,
            &client,
            &registry_path,
            &mut registry,
            &owner_doc,
            &group_id,
            pending_requests,
            self.timeout,
        )?;

        let preview = send_round2_requests(
            &runtime,
            &client,
            &registry_path,
            &mut registry,
            &owner_doc,
            &group_id,
            &collection,
            self.preview,
        )?;

        if let Some((participant_name, ur)) = preview {
            if is_verbose() {
                eprintln!("# Round 2 preview for {}", participant_name);
                eprintln!();
            }
            eprintln!(
                "Collected {} Round 1 packages to {} and sent {} Round 2 requests.",
                collection.packages.len(),
                collection.display_path.display(),
                collection.next_response_arids.len()
            );
            println!("{ur}");
        } else if is_verbose() {
            eprintln!();
            eprintln!(
                "Collected {} Round 1 packages to {} and sent {} Round 2 requests.",
                collection.packages.len(),
                collection.display_path.display(),
                collection.next_response_arids.len()
            );
        } else {
            println!("{}", collection.display_path.display());
        }

        Ok(())
    }
}

struct Round1Collection {
    packages: Vec<(XID, frost::keys::dkg::round1::Package)>,
    next_response_arids: Vec<(XID, ARID)>,
    display_path: PathBuf,
}

#[allow(clippy::too_many_arguments)]
fn collect_round1(
    runtime: &Runtime,
    client: &StorageClient,
    registry_path: &Path,
    registry: &mut Registry,
    owner: &XIDDocument,
    group_id: &ARID,
    pending_requests: &PendingRequests,
    timeout: Option<u64>,
) -> Result<Round1Collection> {
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

    for (participant_xid, collect_from_arid) in pending_requests.iter_collect()
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
            runtime,
            client,
            collect_from_arid,
            timeout,
            owner,
            group_id,
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
            eprintln!("Failed to collect from {} participants:", errors.len());
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
    let packages_dir = group_state_dir(registry_path, group_id);
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
                (
                    xid.ur_string(),
                    serde_json::to_value(package)
                        .expect("Round 1 package serialization"),
                )
            })
            .collect();
    fs::write(
        &round1_packages_path,
        serde_json::to_vec_pretty(&packages_json)?,
    )
    .with_context(|| {
        format!("Failed to write {}", round1_packages_path.display())
    })?;

    // Update pending_requests with the ARIDs where participants want to receive
    // Round 2 requests.
    let mut new_pending = PendingRequests::new();
    for (xid, send_to_arid) in &next_response_arids {
        new_pending.add_send_only(*xid, *send_to_arid);
    }
    let group_record = registry
        .group_mut(group_id)
        .context("Group not found in registry")?;
    group_record.set_pending_requests(new_pending);
    registry.save(registry_path)?;

    let display_path = std::env::current_dir()
        .ok()
        .and_then(|cwd| round1_packages_path.strip_prefix(&cwd).ok())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| round1_packages_path.clone());

    Ok(Round1Collection {
        packages: round1_packages,
        next_response_arids,
        display_path,
    })
}

#[allow(clippy::too_many_arguments)]
fn send_round2_requests(
    runtime: &Runtime,
    client: &StorageClient,
    registry_path: &Path,
    registry: &mut Registry,
    owner: &XIDDocument,
    group_id: &ARID,
    collection: &Round1Collection,
    preview: bool,
) -> Result<Option<(String, String)>> {
    let coordinator_doc = owner;
    let signer_private_keys = coordinator_doc
        .inception_private_keys()
        .context("Coordinator XID document has no signing keys")?;
    let valid_until =
        Date::with_duration_from_now(Duration::from_secs(60 * 60));

    // Build participant info: (XID, XIDDocument, send_to_arid,
    // collect_from_arid)
    let participant_info: Vec<(XID, XIDDocument, ARID, ARID)> = collection
        .next_response_arids
        .iter()
        .map(|(xid, send_to_arid)| {
            let doc = registry
                .participant(xid)
                .map(|r| r.xid_document().clone())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Participant {} not found in registry",
                        xid.ur_string()
                    )
                })?;
            let collect_from_arid = ARID::new();
            Ok((*xid, doc, *send_to_arid, collect_from_arid))
        })
        .collect::<Result<Vec<_>>>()?;

    if is_verbose() {
        eprintln!(
            "Sending Round 2 requests to {} participants...",
            participant_info.len()
        );
    }

    let mut preview_output: Option<(String, String)> = None;

    for (xid, recipient_doc, send_to_arid, collect_from_arid) in
        &participant_info
    {
        let participant_name = registry
            .participant(xid)
            .and_then(|r| r.pet_name().map(|s| s.to_owned()))
            .unwrap_or_else(|| xid.ur_string());

        if is_verbose() {
            eprintln!("{}...", participant_name);
        }

        let request = build_round2_request_for_participant(
            coordinator_doc,
            group_id,
            &collection.packages,
            *collect_from_arid,
        )?;

        if preview && preview_output.is_none() {
            let unsealed_envelope = request.to_envelope(
                Some(valid_until),
                Some(signer_private_keys),
                None,
            )?;
            preview_output =
                Some((participant_name.clone(), unsealed_envelope.ur_string()));
        }

        let sealed_envelope = request.to_envelope_for_recipients(
            Some(valid_until),
            Some(signer_private_keys),
            &[recipient_doc],
        )?;

        runtime.block_on(async {
            client.put(send_to_arid, &sealed_envelope).await
        })?;
    }

    // Build pending requests for Round 2 collection
    let mut new_pending_requests = PendingRequests::new();
    for (xid, _, _, collect_from_arid) in &participant_info {
        new_pending_requests.add_collect_only(*xid, *collect_from_arid);
    }
    let group_record = registry
        .group_mut(group_id)
        .context("Group not found in registry")?;
    group_record.set_pending_requests(new_pending_requests);
    registry.save(registry_path)?;

    Ok(preview_output)
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

fn build_round2_request_for_participant(
    sender: &XIDDocument,
    group_id: &ARID,
    round1_packages: &[(XID, frost::keys::dkg::round1::Package)],
    response_arid: ARID,
) -> Result<SealedRequest> {
    let mut request = SealedRequest::new("dkgRound2", ARID::new(), sender)
        .with_parameter("group", *group_id)
        .with_parameter("responseArid", response_arid);

    // Add all Round 1 packages
    for (xid, package) in round1_packages {
        let encoded = serde_json::to_vec(package)?;
        let json = bc_components::JSON::from_data(encoded);
        let package_envelope =
            Envelope::new(CBOR::from(json)).add_assertion("participant", *xid);
        request = request.with_parameter("round1Package", package_envelope);
    }

    Ok(request)
}

fn group_state_dir(registry_path: &Path, group_id: &ARID) -> PathBuf {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("group-state").join(group_id.hex())
}
