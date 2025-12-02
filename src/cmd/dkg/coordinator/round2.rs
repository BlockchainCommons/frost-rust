use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
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
        dkg::common::{
            OptionalStorageSelector, group_state_dir, parse_arid_ur,
        },
        is_verbose,
        parallel::{CollectionResult, ParallelFetchConfig, parallel_fetch},
        registry::participants_file_path,
        storage::StorageClient,
    },
    registry::{PendingRequests, Registry},
};

/// Collect Round 2 responses and send finalize packages (coordinator).
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

    /// Preview one of the finalize requests while sending
    #[arg(long = "preview")]
    preview: bool,

    /// Use parallel fetch/send with interactive progress display
    #[arg(long)]
    parallel: bool,

    /// Group ID to collect Round 2 responses for
    #[arg(value_name = "GROUP_ID")]
    group_id: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let selection =
            selection.context("Hubert storage is required for round2")?;

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
                "Only the coordinator can collect Round 2 responses and send finalize packages. \
                 Coordinator: {}, Owner: {}",
                group_record.coordinator().xid().ur_string(),
                owner.xid().ur_string()
            );
        }

        let pending_requests = group_record.pending_requests();
        if pending_requests.is_empty() {
            bail!(
                "No pending requests for this group. \
                 Did you run 'frost dkg coordinator round1'?"
            );
        }

        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        if self.parallel {
            // Parallel path with progress display
            let client = Arc::new(client);

            let collection = runtime.block_on(async {
                collect_round2_parallel(
                    Arc::clone(&client),
                    &registry,
                    pending_requests,
                    &owner_doc,
                    &group_id,
                    self.timeout,
                )
                .await
            })?;

            // Persist collected data
            let display_path = persist_round2_packages(
                &registry_path,
                &group_id,
                &collection.successes,
            )?;

            update_pending_for_finalize_from_collection(
                &mut registry,
                &registry_path,
                &group_id,
                &collection.successes,
            )?;

            let preview = runtime.block_on(async {
                dispatch_finalize_requests_parallel(
                    Arc::clone(&client),
                    &mut registry,
                    &registry_path,
                    &owner_doc,
                    &group_id,
                    &collection.successes,
                    self.preview,
                )
                .await
            })?;

            print_summary_parallel(&collection, &display_path, preview);
        } else {
            // Sequential path (original behavior)
            // Phase 1: Collect Round 2 responses
            let collection = collect_round2(
                &runtime,
                &client,
                &registry_path,
                &mut registry,
                &owner_doc,
                &group_id,
                pending_requests,
                self.timeout,
            )?;

            // Phase 2: Send finalize packages
            let preview = send_finalize_requests(
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
                    eprintln!("# Finalize preview for {}", participant_name);
                    eprintln!();
                }
                eprintln!(
                    "Collected {} Round 2 responses to {} and sent {} finalize requests.",
                    collection.packages.len(),
                    collection.display_path.display(),
                    collection.next_response_arids.len()
                );
                println!("{ur}");
            } else if is_verbose() {
                eprintln!();
                eprintln!(
                    "Collected {} Round 2 responses to {} and sent {} finalize requests.",
                    collection.packages.len(),
                    collection.display_path.display(),
                    collection.next_response_arids.len()
                );
            }
        }

        Ok(())
    }
}

struct Round2Collection {
    /// All Round 2 packages: sender XID -> (recipient XID -> package)
    packages: HashMap<XID, Vec<(XID, frost::keys::dkg::round2::Package)>>,
    /// Where each participant wants to receive finalize requests
    next_response_arids: Vec<(XID, ARID)>,
    /// Display path for collected_round2.json
    display_path: PathBuf,
}

#[allow(clippy::too_many_arguments)]
fn collect_round2(
    runtime: &Runtime,
    client: &StorageClient,
    registry_path: &Path,
    registry: &mut Registry,
    owner: &XIDDocument,
    group_id: &ARID,
    pending_requests: &PendingRequests,
    timeout: Option<u64>,
) -> Result<Round2Collection> {
    if is_verbose() {
        eprintln!(
            "Collecting Round 2 responses from {} participants...",
            pending_requests.len()
        );
    }

    let mut all_packages: HashMap<
        XID,
        Vec<(XID, frost::keys::dkg::round2::Package)>,
    > = HashMap::new();
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

        match fetch_round2_response(
            runtime,
            client,
            collect_from_arid,
            timeout,
            owner,
            group_id,
            participant_xid,
        ) {
            Ok(collected) => {
                all_packages.insert(*participant_xid, collected.packages);
                next_response_arids
                    .push((*participant_xid, collected.next_response_arid));
            }
            Err(e) => {
                if is_verbose() {
                    eprintln!("error: {}", e);
                }
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
            "Round 2 collection incomplete: {} of {} responses failed",
            errors.len(),
            pending_requests.len()
        );
    }

    // Persist collected round2 packages keyed by sender XID
    let state_dir = group_state_dir(registry_path, group_id);
    fs::create_dir_all(&state_dir).with_context(|| {
        format!(
            "Failed to create group state directory {}",
            state_dir.display()
        )
    })?;

    let collected_path = state_dir.join("collected_round2.json");
    let mut root = serde_json::Map::new();
    for (sender, packages) in &all_packages {
        let mut sender_map = serde_json::Map::new();
        // Find the response_arid for this sender
        let response_arid = next_response_arids
            .iter()
            .find(|(xid, _)| xid == sender)
            .map(|(_, arid)| arid)
            .expect("sender must have response_arid");
        sender_map.insert(
            "response_arid".to_string(),
            serde_json::Value::String(response_arid.ur_string()),
        );
        let mut packages_json = serde_json::Map::new();
        for (recipient, package) in packages {
            packages_json.insert(
                recipient.ur_string(),
                serde_json::to_value(package).expect("round2 package JSON"),
            );
        }
        sender_map.insert(
            "packages".to_string(),
            serde_json::Value::Object(packages_json),
        );
        root.insert(sender.ur_string(), serde_json::Value::Object(sender_map));
    }
    fs::write(&collected_path, serde_json::to_vec_pretty(&root)?)
        .with_context(|| {
            format!("Failed to write {}", collected_path.display())
        })?;

    // Update pending_requests with the ARIDs where participants want to receive
    // finalize requests.
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
        .and_then(|cwd| collected_path.strip_prefix(&cwd).ok())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| collected_path.clone());

    Ok(Round2Collection {
        packages: all_packages,
        next_response_arids,
        display_path,
    })
}

struct CollectedRound2Entry {
    packages: Vec<(XID, frost::keys::dkg::round2::Package)>,
    next_response_arid: ARID,
}

fn fetch_round2_response(
    runtime: &Runtime,
    client: &StorageClient,
    arid: &ARID,
    timeout: Option<u64>,
    coordinator: &XIDDocument,
    expected_group: &ARID,
    expected_sender: &XID,
) -> Result<CollectedRound2Entry> {
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

    result
        .check_subject_unit()?
        .check_type("dkgRound2Response")?;

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

    Ok(CollectedRound2Entry { packages, next_response_arid })
}

#[allow(clippy::too_many_arguments)]
fn send_finalize_requests(
    runtime: &Runtime,
    client: &StorageClient,
    registry_path: &Path,
    registry: &mut Registry,
    owner: &XIDDocument,
    group_id: &ARID,
    collection: &Round2Collection,
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
            "Sending finalize packages to {} participants...",
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

        // Gather packages FOR this recipient (from all other senders)
        let packages_for_recipient =
            gather_packages_for_recipient(xid, &collection.packages)?;

        let request = build_finalize_request_for_participant(
            coordinator_doc,
            group_id,
            *collect_from_arid,
            &packages_for_recipient,
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

    // Build pending requests for finalize response collection
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

fn gather_packages_for_recipient(
    recipient: &XID,
    all_packages: &HashMap<XID, Vec<(XID, frost::keys::dkg::round2::Package)>>,
) -> Result<Vec<(XID, frost::keys::dkg::round2::Package)>> {
    let mut result = Vec::new();
    for (sender, packages) in all_packages {
        for (rcpt, pkg) in packages {
            if rcpt == recipient {
                result.push((*sender, pkg.clone()));
            }
        }
    }
    if result.is_empty() {
        bail!(
            "No round2 packages found for recipient {}",
            recipient.ur_string()
        );
    }
    Ok(result)
}

/// Build a finalize request for a participant, including the response ARID
/// where they should post their finalize response.
fn build_finalize_request_for_participant(
    sender: &XIDDocument,
    group_id: &ARID,
    response_arid: ARID,
    packages: &[(XID, frost::keys::dkg::round2::Package)],
) -> Result<SealedRequest> {
    let mut request = SealedRequest::new("dkgFinalize", ARID::new(), sender)
        .with_parameter("group", *group_id)
        .with_parameter("responseArid", response_arid);

    for (pkg_sender, package) in packages {
        let encoded = serde_json::to_vec(package)?;
        let json = bc_components::JSON::from_data(encoded);
        let pkg_envelope = Envelope::new(CBOR::from(json))
            .add_assertion("sender", *pkg_sender);
        request = request.with_parameter("round2Package", pkg_envelope);
    }

    Ok(request)
}

// -----------------------------------------------------------------------------
// Parallel implementations
// -----------------------------------------------------------------------------

/// Data extracted from a successful Round 2 response.
struct Round2ResponseData {
    packages: Vec<(XID, frost::keys::dkg::round2::Package)>,
    next_response_arid: ARID,
}

/// Collect Round 2 responses in parallel with progress display.
async fn collect_round2_parallel(
    client: Arc<StorageClient>,
    registry: &Registry,
    pending_requests: &PendingRequests,
    coordinator: &XIDDocument,
    expected_group_id: &ARID,
    timeout: Option<u64>,
) -> Result<CollectionResult<Round2ResponseData>> {
    let requests: Vec<(XID, ARID, String)> = pending_requests
        .iter_collect()
        .map(|(xid, arid)| {
            let name = registry
                .participant(xid)
                .and_then(|r| r.pet_name().map(|s| s.to_owned()))
                .unwrap_or_else(|| xid.ur_string());
            (*xid, *arid, name)
        })
        .collect();

    let coordinator_keys = coordinator
        .inception_private_keys()
        .context("Missing coordinator private keys")?
        .clone();
    let group_id = *expected_group_id;

    let config = ParallelFetchConfig::with_timeout(timeout);

    parallel_fetch(client, requests, config, move |envelope, xid| {
        validate_and_extract_round2_response(
            envelope,
            &coordinator_keys,
            &group_id,
            xid,
        )
    })
    .await
}

/// Validate envelope and extract Round 2 data (for parallel fetch).
fn validate_and_extract_round2_response(
    envelope: &Envelope,
    coordinator_keys: &bc_components::PrivateKeys,
    expected_group_id: &ARID,
    expected_sender: &XID,
) -> Result<Round2ResponseData> {
    let now = Date::now();
    let sealed = SealedResponse::try_from_encrypted_envelope(
        envelope,
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

    result
        .check_subject_unit()?
        .check_type("dkgRound2Response")?;

    let group_id: ARID = result.extract_object_for_predicate("group")?;
    if &group_id != expected_group_id {
        bail!(
            "Response group ID {} does not match expected {}",
            group_id.ur_string(),
            expected_group_id.ur_string()
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

    Ok(Round2ResponseData { packages, next_response_arid })
}

/// Persist Round 2 packages from parallel collection results.
fn persist_round2_packages(
    registry_path: &Path,
    group_id: &ARID,
    successes: &[(XID, Round2ResponseData)],
) -> Result<PathBuf> {
    let state_dir = group_state_dir(registry_path, group_id);
    fs::create_dir_all(&state_dir).with_context(|| {
        format!(
            "Failed to create group state directory {}",
            state_dir.display()
        )
    })?;

    let collected_path = state_dir.join("collected_round2.json");
    let mut root = serde_json::Map::new();
    for (sender, data) in successes {
        let mut sender_map = serde_json::Map::new();
        sender_map.insert(
            "response_arid".to_string(),
            serde_json::Value::String(data.next_response_arid.ur_string()),
        );
        let mut packages_json = serde_json::Map::new();
        for (recipient, package) in &data.packages {
            packages_json.insert(
                recipient.ur_string(),
                serde_json::to_value(package).expect("round2 package JSON"),
            );
        }
        sender_map.insert(
            "packages".to_string(),
            serde_json::Value::Object(packages_json),
        );
        root.insert(sender.ur_string(), serde_json::Value::Object(sender_map));
    }
    fs::write(&collected_path, serde_json::to_vec_pretty(&root)?)
        .with_context(|| {
            format!("Failed to write {}", collected_path.display())
        })?;

    let display_path = std::env::current_dir()
        .ok()
        .and_then(|cwd| collected_path.strip_prefix(&cwd).ok())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| collected_path.clone());

    Ok(display_path)
}

/// Update pending requests from parallel collection results.
fn update_pending_for_finalize_from_collection(
    registry: &mut Registry,
    registry_path: &Path,
    group_id: &ARID,
    successes: &[(XID, Round2ResponseData)],
) -> Result<()> {
    let mut new_pending = PendingRequests::new();
    for (xid, data) in successes {
        new_pending.add_send_only(*xid, data.next_response_arid);
    }
    let group_record = registry
        .group_mut(group_id)
        .context("Group not found in registry")?;
    group_record.set_pending_requests(new_pending);
    registry.save(registry_path)?;
    Ok(())
}

/// Dispatch finalize requests in parallel.
async fn dispatch_finalize_requests_parallel(
    client: Arc<StorageClient>,
    registry: &mut Registry,
    registry_path: &Path,
    coordinator: &XIDDocument,
    group_id: &ARID,
    successes: &[(XID, Round2ResponseData)],
    preview: bool,
) -> Result<Option<(String, String)>> {
    use crate::cmd::parallel::parallel_send;

    let signer_private_keys = coordinator
        .inception_private_keys()
        .context("Coordinator XID document has no signing keys")?;
    let valid_until =
        Date::with_duration_from_now(Duration::from_secs(60 * 60));

    // Build all_packages map for gather_packages_for_recipient
    let all_packages: HashMap<
        XID,
        Vec<(XID, frost::keys::dkg::round2::Package)>,
    > = successes
        .iter()
        .map(|(xid, data)| (*xid, data.packages.clone()))
        .collect();

    // Build messages
    let mut messages: Vec<(XID, ARID, Envelope, String)> = Vec::new();
    let mut collect_arids: Vec<(XID, ARID)> = Vec::new();
    let mut preview_output: Option<(String, String)> = None;

    for (xid, data) in successes {
        let recipient_doc = registry
            .participant(xid)
            .map(|r| r.xid_document().clone())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Participant {} not found in registry",
                    xid.ur_string()
                )
            })?;

        let participant_name = registry
            .participant(xid)
            .and_then(|r| r.pet_name().map(|s| s.to_owned()))
            .unwrap_or_else(|| xid.ur_string());

        let collect_from_arid = ARID::new();
        collect_arids.push((*xid, collect_from_arid));

        let packages_for_recipient =
            gather_packages_for_recipient(xid, &all_packages)?;

        let request = build_finalize_request_for_participant(
            coordinator,
            group_id,
            collect_from_arid,
            &packages_for_recipient,
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
            &[&recipient_doc],
        )?;

        messages.push((
            *xid,
            data.next_response_arid,
            sealed_envelope,
            participant_name,
        ));
    }

    // Send all messages in parallel
    let send_results = parallel_send(client, messages).await;

    // Check for send failures
    let failures: Vec<_> = send_results
        .iter()
        .filter_map(|(xid, result)| {
            result.as_ref().err().map(|e| (*xid, e.to_string()))
        })
        .collect();

    if !failures.is_empty() {
        for (xid, error) in &failures {
            eprintln!("Failed to send to {}: {}", xid.ur_string(), error);
        }
        bail!(
            "Failed to send finalize requests to {} participants",
            failures.len()
        );
    }

    // Update pending requests for finalize response collection
    let mut new_pending_requests = PendingRequests::new();
    for (xid, collect_from_arid) in &collect_arids {
        new_pending_requests.add_collect_only(*xid, *collect_from_arid);
    }
    let group_record = registry
        .group_mut(group_id)
        .context("Group not found in registry")?;
    group_record.set_pending_requests(new_pending_requests);
    registry.save(registry_path)?;

    Ok(preview_output)
}

/// Print summary for parallel collection.
fn print_summary_parallel(
    collection: &CollectionResult<Round2ResponseData>,
    display_path: &Path,
    preview: Option<(String, String)>,
) {
    // Report any failures
    if !collection.rejections.is_empty() {
        eprintln!();
        eprintln!("Rejections:");
        for (xid, reason) in &collection.rejections {
            eprintln!("  {}: {}", xid.ur_string(), reason);
        }
    }
    if !collection.errors.is_empty() {
        eprintln!();
        eprintln!("Errors:");
        for (xid, error) in &collection.errors {
            eprintln!("  {}: {}", xid.ur_string(), error);
        }
    }
    if !collection.timeouts.is_empty() {
        eprintln!();
        eprintln!("Timeouts:");
        for xid in &collection.timeouts {
            eprintln!("  {}", xid.ur_string());
        }
    }

    if !collection.all_succeeded() {
        eprintln!();
        eprintln!(
            "Round 2 collection incomplete: {} succeeded, {} rejected, {} errors, {} timeouts",
            collection.successes.len(),
            collection.rejections.len(),
            collection.errors.len(),
            collection.timeouts.len()
        );
        return;
    }

    if let Some((participant_name, ur)) = preview {
        if is_verbose() {
            eprintln!("# Finalize preview for {}", participant_name);
            eprintln!();
        }
        eprintln!(
            "Collected {} Round 2 responses to {} and sent {} finalize requests.",
            collection.successes.len(),
            display_path.display(),
            collection.successes.len()
        );
        println!("{ur}");
    } else if is_verbose() {
        eprintln!();
        eprintln!(
            "Collected {} Round 2 responses to {} and sent {} finalize requests.",
            collection.successes.len(),
            display_path.display(),
            collection.successes.len()
        );
    }
}
