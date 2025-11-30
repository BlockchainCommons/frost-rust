use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, JSON, XID, XIDProvider};
use bc_envelope::prelude::*;
use bc_xid::XIDDocument;
use clap::Parser;
use frost_ed25519 as frost;
use gstp::{SealedResponse, SealedResponseBehavior};
use tokio::runtime::Runtime;

use crate::{
    cmd::{
        dkg::{OptionalStorageSelector, common::parse_arid_ur},
        is_verbose,
        registry::participants_file_path,
        storage::StorageClient,
    },
    registry::Registry,
};

/// Collect signCommit responses and dispatch signShare requests (coordinator).
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

    /// Optional group ID to constrain lookup when multiple groups exist
    #[arg(long = "group", value_name = "UR:ARID")]
    group_id: Option<String>,

    /// Print a sample unsealed signShare request (does not affect sending)
    #[arg(long = "preview-share")]
    preview_share: bool,

    /// Signing session ID to collect
    #[arg(value_name = "SESSION_ID")]
    session_id: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let selection =
            selection.context("Hubert storage is required for sign collect")?;

        let registry_path = participants_file_path(self.registry.clone())?;
        let registry = Registry::load(&registry_path).with_context(|| {
            format!("Failed to load registry at {}", registry_path.display())
        })?;

        let owner = registry
            .owner()
            .context("Registry owner is required")?
            .clone();

        let session_id = parse_arid_ur(&self.session_id)?;
        let group_hint = match self.group_id {
            Some(raw) => Some(parse_arid_ur(&raw)?),
            None => None,
        };
        let start_state =
            load_start_state(&registry_path, &session_id, group_hint)?;
        let group_id = start_state.group_id;
        let group_record = registry
            .group(&group_id)
            .context("Group not found in registry")?
            .clone();

        if group_record.coordinator().xid() != &owner.xid() {
            bail!(
                "Only the coordinator can collect signCommit responses. \
                 Coordinator: {}, Owner: {}",
                group_record.coordinator().xid().ur_string(),
                owner.xid().ur_string()
            );
        }

        if is_verbose() {
            eprintln!(
                "Collecting signCommit responses for session {} from {} participants...",
                session_id.ur_string(),
                start_state.participants.len()
            );
        }

        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        let mut commitments: BTreeMap<XID, frost::round1::SigningCommitments> =
            BTreeMap::new();
        let mut send_to_arids: BTreeMap<XID, ARID> = BTreeMap::new();
        let mut errors: Vec<(XID, String)> = Vec::new();

        for (participant, participant_state) in &start_state.participants {
            let participant_name = registry
                .participant(participant)
                .and_then(|r| r.pet_name().map(|s| s.to_owned()))
                .unwrap_or_else(|| participant.ur_string());

            if is_verbose() {
                eprintln!("{}...", participant_name);
            }

            match fetch_commit_response(
                &runtime,
                &client,
                &participant_state.commit_arid,
                self.timeout,
                owner.xid_document(),
                participant,
                &session_id,
            ) {
                Ok((participant_commitments, next_request_arid)) => {
                    commitments.insert(*participant, participant_commitments);
                    send_to_arids.insert(*participant, next_request_arid);
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    errors.push((*participant, e.to_string()));
                }
            }
        }

        if !errors.is_empty() {
            bail!(
                "Sign commit collection incomplete: {} of {} responses failed",
                errors.len(),
                start_state.participants.len()
            );
        }

        if commitments.len() != start_state.participants.len() {
            let missing: Vec<String> = start_state
                .participants
                .keys()
                .filter(|xid| !commitments.contains_key(*xid))
                .map(|xid| xid.ur_string())
                .collect();
            bail!("Missing signCommit responses from: {}", missing.join(", "));
        }

        // Persist aggregated commitments for this session
        let signing_dir =
            signing_state_dir(&registry_path, &group_id, &session_id);
        fs::create_dir_all(&signing_dir).with_context(|| {
            format!(
                "Failed to create signing state directory {}",
                signing_dir.display()
            )
        })?;

        let commitments_path = signing_dir.join("commitments.json");
        let mut commitments_json = serde_json::Map::new();
        for (xid, commits) in &commitments {
            let participant_state = start_state.participants.get(xid).expect(
                "participant present in start state after earlier validation",
            );

            let mut entry = serde_json::Map::new();
            entry.insert(
                "commitments".to_string(),
                serde_json::to_value(commits)
                    .context("Failed to serialize commitments")?,
            );
            entry.insert(
                "share_arid".to_string(),
                serde_json::Value::String(
                    participant_state.share_arid.ur_string(),
                ),
            );

            commitments_json
                .insert(xid.ur_string(), serde_json::Value::Object(entry));
        }

        let mut root = serde_json::Map::new();
        root.insert(
            "group".to_string(),
            serde_json::Value::String(group_id.ur_string()),
        );
        root.insert(
            "session".to_string(),
            serde_json::Value::String(session_id.ur_string()),
        );
        root.insert(
            "target".to_string(),
            serde_json::Value::String(start_state.target_ur.clone()),
        );
        root.insert(
            "commitments".to_string(),
            serde_json::Value::Object(commitments_json),
        );

        fs::write(&commitments_path, serde_json::to_vec_pretty(&root)?)
            .with_context(|| {
                format!("Failed to write {}", commitments_path.display())
            })?;

        // Build and send signShare requests
        let signer_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Coordinator XID document has no signing keys")?;
        let valid_until =
            Date::with_duration_from_now(Duration::from_secs(60 * 60));

        if is_verbose() {
            eprintln!(
                "Dispatching signShare requests to {} participants...",
                send_to_arids.len()
            );
        }

        let mut preview_printed = false;
        for (participant, send_to_arid) in &send_to_arids {
            let participant_state =
                start_state.participants.get(participant).expect(
                    "participant present in start state after earlier validation",
                );

            let recipient_doc = if *participant == owner.xid() {
                owner.xid_document().clone()
            } else {
                registry
                    .participant(participant)
                    .map(|r| r.xid_document().clone())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Participant {} not found in registry",
                            participant.ur_string()
                        )
                    })?
            };

            let request = build_sign_share_request(
                owner.xid_document(),
                &group_id,
                &session_id,
                participant_state.share_arid,
                &commitments,
            )?;

            if self.preview_share && !preview_printed {
                let preview = request.to_envelope(
                    Some(valid_until),
                    Some(signer_keys),
                    None,
                )?;
                println!("# signShare preview for {}", participant.ur_string());
                println!("{}", preview.format());
                preview_printed = true;
            }

            let sealed_envelope = request.to_envelope_for_recipients(
                Some(valid_until),
                Some(signer_keys),
                &[&recipient_doc],
            )?;

            runtime.block_on(async {
                client.put(send_to_arid, &sealed_envelope).await
            })?;
        }

        let display_path = std::env::current_dir()
            .ok()
            .and_then(|cwd| commitments_path.strip_prefix(&cwd).ok())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| commitments_path.clone());

        if is_verbose() {
            eprintln!();
            eprintln!(
                "Collected {} signCommit responses. Saved to {}",
                commitments.len(),
                display_path.display()
            );
            eprintln!("Dispatched {} signShare requests.", commitments.len());
        } else {
            println!("{}", display_path.display());
        }

        Ok(())
    }
}

fn fetch_commit_response(
    runtime: &Runtime,
    client: &StorageClient,
    response_arid: &ARID,
    timeout: Option<u64>,
    coordinator: &XIDDocument,
    expected_sender: &XID,
    expected_session_id: &ARID,
) -> Result<(frost::round1::SigningCommitments, ARID)> {
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

    if sealed_response.sender().xid() != *expected_sender {
        bail!(
            "Unexpected response sender: {} (expected {})",
            sealed_response.sender().xid().ur_string(),
            expected_sender.ur_string()
        );
    }

    if let Ok(error) = sealed_response.error() {
        let reason = error
            .object_for_predicate("reason")
            .ok()
            .and_then(|e| e.extract_subject::<String>().ok())
            .unwrap_or_else(|| "unknown reason".to_string());
        bail!("Participant rejected signCommit: {}", reason);
    }

    let result = sealed_response
        .result()
        .context("Response has no result envelope")?;

    let function: String = result.extract_subject()?;
    if function != "signCommitResponse" {
        bail!("Unexpected response function: {}", function);
    }

    let response_session: ARID =
        result.extract_object_for_predicate("session")?;
    if response_session != *expected_session_id {
        bail!(
            "Response session {} does not match expected {}",
            response_session.ur_string(),
            expected_session_id.ur_string()
        );
    }

    let commitments_json: JSON =
        result.extract_object_for_predicate("commitments")?;
    let commitments: frost::round1::SigningCommitments =
        serde_json::from_slice(commitments_json.as_bytes())
            .context("Failed to deserialize commitments")?;

    let next_request_arid: ARID =
        result.extract_object_for_predicate("response_arid")?;

    Ok((commitments, next_request_arid))
}

fn build_sign_share_request(
    sender: &XIDDocument,
    _group_id: &ARID,
    session_id: &ARID,
    response_arid: ARID,
    commitments: &BTreeMap<XID, frost::round1::SigningCommitments>,
) -> Result<gstp::SealedRequest> {
    let mut request =
        gstp::SealedRequest::new("signShare", *session_id, sender)
            .with_parameter("session", *session_id)
            .with_parameter("response_arid", response_arid);

    for (participant, commits) in commitments {
        let commits_json = JSON::from_data(serde_json::to_vec(commits)?);
        let entry = Envelope::new(*participant)
            .add_assertion("commitments", CBOR::from(commits_json));
        request = request.with_parameter("commitment", entry);
    }

    Ok(request)
}

fn load_start_state(
    registry_path: &Path,
    session_id: &ARID,
    group_hint: Option<ARID>,
) -> Result<StartState> {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let group_state_dir = base.join("group-state");

    let mut candidate_paths = Vec::new();
    let group_dirs: Vec<(ARID, PathBuf)> = if let Some(group) = group_hint {
        vec![(group, group_state_dir.join(group.hex()))]
    } else {
        let mut dirs = Vec::new();
        if group_state_dir.exists() {
            for entry in fs::read_dir(&group_state_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    let dir_name = entry.file_name();
                    if let Some(name) = dir_name.to_str()
                        && name.len() == 64
                        && name.chars().all(|c| c.is_ascii_hexdigit())
                    {
                        let group_id = ARID::from_hex(name);
                        dirs.push((group_id, entry.path()));
                    }
                }
            }
        }
        dirs
    };

    for (group_id, group_dir) in group_dirs {
        let candidate = group_dir
            .join("signing")
            .join(session_id.hex())
            .join("start.json");
        if candidate.exists() {
            candidate_paths.push((group_id, candidate));
        }
    }

    if candidate_paths.is_empty() {
        bail!(
            "No sign start state found; run `frost sign coordinator start` first"
        );
    }
    if candidate_paths.len() > 1 {
        bail!(
            "Multiple signing sessions found; specify --session to disambiguate"
        );
    }

    let (group_id, path) = &candidate_paths[0];
    let raw: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(
            &fs::read(path).with_context(|| {
                format!("Failed to read {}", path.display())
            })?,
        )
        .context("Invalid start.json")?;

    let get_str = |key: &str| -> Result<String> {
        raw.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .with_context(|| format!("Missing or invalid {key} in start.json"))
    };

    let session_in_state = parse_arid_ur(&get_str("session_id")?)?;
    let group_in_state = parse_arid_ur(&get_str("group")?)?;
    if session_in_state != *session_id {
        bail!(
            "start.json session {} does not match requested session {}",
            session_in_state.ur_string(),
            session_id.ur_string()
        );
    }
    if group_in_state != *group_id {
        bail!(
            "start.json group {} does not match directory group {}",
            group_in_state.ur_string(),
            group_id.ur_string()
        );
    }
    let target_ur = get_str("target")?;

    let participants_val = raw
        .get("participants")
        .and_then(|v| v.as_object())
        .context("Missing participants in start.json")?;

    let mut participants = HashMap::new();
    for (xid_str, value) in participants_val {
        let xid = XID::from_ur_string(xid_str)
            .context("Invalid participant XID in start.json")?;
        let obj = value
            .as_object()
            .context("Participant entry is not an object in start.json")?;

        let commit_arid = obj
            .get("commit_arid")
            .and_then(|v| v.as_str())
            .context("Missing commit_arid in start.json")?;
        let share_arid = obj
            .get("share_arid")
            .and_then(|v| v.as_str())
            .context("Missing share_arid in start.json")?;

        participants.insert(
            xid,
            StartParticipant {
                commit_arid: parse_arid_ur(commit_arid)?,
                share_arid: parse_arid_ur(share_arid)?,
            },
        );
    }

    Ok(StartState { group_id: *group_id, target_ur, participants })
}

struct StartParticipant {
    commit_arid: ARID,
    share_arid: ARID,
}

struct StartState {
    group_id: ARID,
    target_ur: String,
    participants: HashMap<XID, StartParticipant>,
}

fn signing_state_dir_for_group(
    registry_path: &Path,
    group_id: &ARID,
) -> PathBuf {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("group-state")
        .join(group_id.hex())
        .join("signing")
}

fn signing_state_dir(
    registry_path: &Path,
    group_id: &ARID,
    session_id: &ARID,
) -> PathBuf {
    signing_state_dir_for_group(registry_path, group_id).join(session_id.hex())
}
