use std::{
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

    /// Use parallel fetch/send with interactive progress display
    #[arg(long)]
    parallel: bool,

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

        validate_coordinator(&group_record, &owner)?;

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

        if self.parallel {
            // Parallel path with progress display
            let client = Arc::new(client);
            let collection = runtime.block_on(async {
                collect_round1_responses_parallel(
                    Arc::clone(&client),
                    &registry,
                    pending_requests,
                    &owner_doc,
                    &group_id,
                    self.timeout,
                )
                .await
            })?;

            // Extract packages for persistence
            let packages: Vec<(XID, frost::keys::dkg::round1::Package)> =
                collection
                    .successes
                    .iter()
                    .map(|(xid, data)| (*xid, data.package.clone()))
                    .collect();

            let display_path =
                persist_round1_packages(&registry_path, &group_id, &packages)?;

            update_pending_for_round2_from_collection(
                &mut registry,
                &registry_path,
                &group_id,
                &collection.successes,
            )?;

            let preview = runtime.block_on(async {
                dispatch_round2_requests_parallel(
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
            let mut ctx = Round1Context {
                runtime: &runtime,
                client: &client,
                registry_path: &registry_path,
                registry: &mut registry,
                owner_doc: &owner_doc,
                group_id: &group_id,
            };

            let collection = collect_round1_responses(
                &mut ctx,
                pending_requests,
                self.timeout,
            )?;

            let preview =
                dispatch_round2_requests(&mut ctx, &collection, self.preview)?;

            print_summary(&collection, preview);
        }

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Context and result types
// -----------------------------------------------------------------------------

struct Round1Context<'a> {
    runtime: &'a Runtime,
    client: &'a StorageClient,
    registry_path: &'a Path,
    registry: &'a mut Registry,
    owner_doc: &'a XIDDocument,
    group_id: &'a ARID,
}

type Round1Package = (XID, frost::keys::dkg::round1::Package);
type NextResponseArid = (XID, ARID);

struct Round1Collection {
    packages: Vec<Round1Package>,
    next_response_arids: Vec<NextResponseArid>,
    display_path: PathBuf,
}

// -----------------------------------------------------------------------------
// Validation
// -----------------------------------------------------------------------------

fn validate_coordinator(
    group_record: &crate::registry::GroupRecord,
    owner: &crate::registry::OwnerRecord,
) -> Result<()> {
    if group_record.coordinator().xid() != &owner.xid() {
        bail!(
            "Only the coordinator can collect and send Round 2 requests. \
             Coordinator: {}, Owner: {}",
            group_record.coordinator().xid().ur_string(),
            owner.xid().ur_string()
        );
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// Round 1 collection
// -----------------------------------------------------------------------------

fn collect_round1_responses(
    ctx: &mut Round1Context<'_>,
    pending_requests: &PendingRequests,
    timeout: Option<u64>,
) -> Result<Round1Collection> {
    if is_verbose() {
        eprintln!(
            "Collecting Round 1 responses from {} participants...",
            pending_requests.len()
        );
    }

    let (packages, next_response_arids) =
        fetch_all_round1_packages(ctx, pending_requests, timeout)?;

    let display_path =
        persist_round1_packages(ctx.registry_path, ctx.group_id, &packages)?;

    update_pending_for_round2(ctx, &next_response_arids)?;

    Ok(Round1Collection { packages, next_response_arids, display_path })
}

fn fetch_all_round1_packages(
    ctx: &Round1Context<'_>,
    pending_requests: &PendingRequests,
    timeout: Option<u64>,
) -> Result<(Vec<Round1Package>, Vec<NextResponseArid>)> {
    let mut round1_packages = Vec::new();
    let mut next_response_arids = Vec::new();
    let mut errors: Vec<(XID, String)> = Vec::new();

    for (participant_xid, collect_from_arid) in pending_requests.iter_collect()
    {
        let participant_name = ctx
            .registry
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
            ctx.runtime,
            ctx.client,
            collect_from_arid,
            timeout,
            ctx.owner_doc,
            ctx.group_id,
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

    Ok((round1_packages, next_response_arids))
}

fn persist_round1_packages(
    registry_path: &Path,
    group_id: &ARID,
    packages: &[(XID, frost::keys::dkg::round1::Package)],
) -> Result<PathBuf> {
    let packages_dir = group_state_dir(registry_path, group_id);
    fs::create_dir_all(&packages_dir).with_context(|| {
        format!(
            "Failed to create group state directory {}",
            packages_dir.display()
        )
    })?;

    let round1_packages_path = packages_dir.join("collected_round1.json");
    let packages_json: serde_json::Map<String, serde_json::Value> = packages
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

    let display_path = std::env::current_dir()
        .ok()
        .and_then(|cwd| round1_packages_path.strip_prefix(&cwd).ok())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| round1_packages_path.clone());

    Ok(display_path)
}

fn update_pending_for_round2(
    ctx: &mut Round1Context<'_>,
    next_response_arids: &[(XID, ARID)],
) -> Result<()> {
    let mut new_pending = PendingRequests::new();
    for (xid, send_to_arid) in next_response_arids {
        new_pending.add_send_only(*xid, *send_to_arid);
    }
    let group_record = ctx
        .registry
        .group_mut(ctx.group_id)
        .context("Group not found in registry")?;
    group_record.set_pending_requests(new_pending);
    ctx.registry.save(ctx.registry_path)?;
    Ok(())
}

// -----------------------------------------------------------------------------
// Round 2 dispatch
// -----------------------------------------------------------------------------

fn dispatch_round2_requests(
    ctx: &mut Round1Context<'_>,
    collection: &Round1Collection,
    preview: bool,
) -> Result<Option<(String, String)>> {
    let signer_private_keys = ctx
        .owner_doc
        .inception_private_keys()
        .context("Coordinator XID document has no signing keys")?;
    let valid_until =
        Date::with_duration_from_now(Duration::from_secs(60 * 60));

    let participant_info = build_round2_participant_info(
        ctx.registry,
        &collection.next_response_arids,
    )?;

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
        let participant_name = ctx
            .registry
            .participant(xid)
            .and_then(|r| r.pet_name().map(|s| s.to_owned()))
            .unwrap_or_else(|| xid.ur_string());

        if is_verbose() {
            eprintln!("{}...", participant_name);
        }

        let request = build_round2_request_for_participant(
            ctx.owner_doc,
            ctx.group_id,
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

        ctx.runtime.block_on(async {
            ctx.client.put(send_to_arid, &sealed_envelope).await
        })?;
    }

    update_pending_for_round2_collection(ctx, &participant_info)?;

    Ok(preview_output)
}

fn build_round2_participant_info(
    registry: &Registry,
    next_response_arids: &[(XID, ARID)],
) -> Result<Vec<(XID, XIDDocument, ARID, ARID)>> {
    next_response_arids
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
        .collect()
}

fn update_pending_for_round2_collection(
    ctx: &mut Round1Context<'_>,
    participant_info: &[(XID, XIDDocument, ARID, ARID)],
) -> Result<()> {
    let mut new_pending_requests = PendingRequests::new();
    for (xid, _, _, collect_from_arid) in participant_info {
        new_pending_requests.add_collect_only(*xid, *collect_from_arid);
    }
    let group_record = ctx
        .registry
        .group_mut(ctx.group_id)
        .context("Group not found in registry")?;
    group_record.set_pending_requests(new_pending_requests);
    ctx.registry.save(ctx.registry_path)?;
    Ok(())
}

// -----------------------------------------------------------------------------
// Output
// -----------------------------------------------------------------------------

fn print_summary(
    collection: &Round1Collection,
    preview: Option<(String, String)>,
) {
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
}

// -----------------------------------------------------------------------------
// GSTP response handling
// -----------------------------------------------------------------------------

fn fetch_and_validate_response(
    runtime: &Runtime,
    client: &StorageClient,
    response_arid: &ARID,
    timeout: Option<u64>,
    coordinator: &XIDDocument,
    expected_group_id: &ARID,
) -> Result<(frost::keys::dkg::round1::Package, ARID)> {
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

    let now = Date::now();
    let sealed_response = SealedResponse::try_from_encrypted_envelope(
        &envelope,
        None,
        Some(now),
        coordinator_private_keys,
    )?;

    if let Ok(error) = sealed_response.error() {
        let reason = error
            .object_for_predicate("reason")
            .ok()
            .and_then(|e| e.extract_subject::<String>().ok())
            .unwrap_or_else(|| "unknown reason".to_string());
        bail!("Participant rejected invite: {}", reason);
    }

    let result = sealed_response
        .result()
        .context("Response has no result envelope")?;

    validate_round1_response(result, expected_group_id)?;

    let next_response_arid: ARID =
        result.extract_object_for_predicate("response_arid")?;

    let round1_package = extract_round1_package(result)?;

    Ok((round1_package, next_response_arid))
}

fn validate_round1_response(
    result: &Envelope,
    expected_group_id: &ARID,
) -> Result<()> {
    result
        .check_subject_unit()?
        .check_type("dkgRound1Response")?;

    let group_id: ARID = result.extract_object_for_predicate("group")?;
    if group_id != *expected_group_id {
        bail!(
            "Response group ID {} does not match expected {}",
            group_id.ur_string(),
            expected_group_id.ur_string()
        );
    }

    Ok(())
}

fn extract_round1_package(
    result: &Envelope,
) -> Result<frost::keys::dkg::round1::Package> {
    let round1_envelope: Envelope =
        result.object_for_predicate("round1_package")?;
    let round1_json: bc_components::JSON = round1_envelope
        .extract_subject()
        .context("round1_package missing")?;
    serde_json::from_slice(round1_json.as_bytes())
        .context("Failed to deserialize Round 1 package")
}

// -----------------------------------------------------------------------------
// Round 2 request building
// -----------------------------------------------------------------------------

fn build_round2_request_for_participant(
    sender: &XIDDocument,
    group_id: &ARID,
    round1_packages: &[(XID, frost::keys::dkg::round1::Package)],
    response_arid: ARID,
) -> Result<SealedRequest> {
    let mut request = SealedRequest::new("dkgRound2", ARID::new(), sender)
        .with_parameter("group", *group_id)
        .with_parameter("responseArid", response_arid);

    for (xid, package) in round1_packages {
        let encoded = serde_json::to_vec(package)?;
        let json = bc_components::JSON::from_data(encoded);
        let package_envelope =
            Envelope::new(CBOR::from(json)).add_assertion("participant", *xid);
        request = request.with_parameter("round1Package", package_envelope);
    }

    Ok(request)
}

// -----------------------------------------------------------------------------
// Parallel implementations
// -----------------------------------------------------------------------------

/// Data extracted from a successful Round 1 response.
struct Round1ResponseData {
    package: frost::keys::dkg::round1::Package,
    next_response_arid: ARID,
}

/// Collect Round 1 responses in parallel with progress display.
async fn collect_round1_responses_parallel(
    client: Arc<StorageClient>,
    registry: &Registry,
    pending_requests: &PendingRequests,
    coordinator: &XIDDocument,
    expected_group_id: &ARID,
    timeout: Option<u64>,
) -> Result<CollectionResult<Round1ResponseData>> {
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

    parallel_fetch(client, requests, config, move |envelope, _xid| {
        validate_and_extract_round1_response(
            envelope,
            &coordinator_keys,
            &group_id,
        )
    })
    .await
}

/// Validate envelope and extract Round 1 data (for parallel fetch).
fn validate_and_extract_round1_response(
    envelope: &Envelope,
    coordinator_keys: &bc_components::PrivateKeys,
    expected_group_id: &ARID,
) -> Result<Round1ResponseData> {
    let now = Date::now();
    let sealed_response = SealedResponse::try_from_encrypted_envelope(
        envelope,
        None,
        Some(now),
        coordinator_keys,
    )?;

    if let Ok(error) = sealed_response.error() {
        let reason = error
            .object_for_predicate("reason")
            .ok()
            .and_then(|e| e.extract_subject::<String>().ok())
            .unwrap_or_else(|| "unknown reason".to_string());
        bail!("Participant rejected invite: {}", reason);
    }

    let result = sealed_response
        .result()
        .context("Response has no result envelope")?;

    validate_round1_response(result, expected_group_id)?;

    let next_response_arid: ARID =
        result.extract_object_for_predicate("response_arid")?;
    let package = extract_round1_package(result)?;

    Ok(Round1ResponseData { package, next_response_arid })
}

/// Update pending requests from parallel collection results.
fn update_pending_for_round2_from_collection(
    registry: &mut Registry,
    registry_path: &Path,
    group_id: &ARID,
    successes: &[(XID, Round1ResponseData)],
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

/// Dispatch Round 2 requests in parallel.
async fn dispatch_round2_requests_parallel(
    client: Arc<StorageClient>,
    registry: &mut Registry,
    registry_path: &Path,
    coordinator: &XIDDocument,
    group_id: &ARID,
    successes: &[(XID, Round1ResponseData)],
    preview: bool,
) -> Result<Option<(String, String)>> {
    use crate::cmd::parallel::parallel_send;

    let signer_private_keys = coordinator
        .inception_private_keys()
        .context("Coordinator XID document has no signing keys")?;
    let valid_until =
        Date::with_duration_from_now(Duration::from_secs(60 * 60));

    // Build round1 packages list for request building
    let round1_packages: Vec<(XID, frost::keys::dkg::round1::Package)> =
        successes
            .iter()
            .map(|(xid, data)| (*xid, data.package.clone()))
            .collect();

    // Build participant info and messages
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

        let request = build_round2_request_for_participant(
            coordinator,
            group_id,
            &round1_packages,
            collect_from_arid,
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
            "Failed to send Round 2 requests to {} participants",
            failures.len()
        );
    }

    // Update pending requests for Round 2 collection
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
    collection: &CollectionResult<Round1ResponseData>,
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
        bail_with_collection_summary(collection);
    }

    if let Some((participant_name, ur)) = preview {
        if is_verbose() {
            eprintln!("# Round 2 preview for {}", participant_name);
            eprintln!();
        }
        eprintln!(
            "Collected {} Round 1 packages to {} and sent {} Round 2 requests.",
            collection.successes.len(),
            display_path.display(),
            collection.successes.len()
        );
        println!("{ur}");
    } else if is_verbose() {
        eprintln!();
        eprintln!(
            "Collected {} Round 1 packages to {} and sent {} Round 2 requests.",
            collection.successes.len(),
            display_path.display(),
            collection.successes.len()
        );
    } else {
        println!("{}", display_path.display());
    }
}

fn bail_with_collection_summary(
    collection: &CollectionResult<Round1ResponseData>,
) {
    eprintln!(
        "Round 1 collection incomplete: {} succeeded, {} rejected, {} errors, {} timeouts",
        collection.successes.len(),
        collection.rejections.len(),
        collection.errors.len(),
        collection.timeouts.len()
    );
}
