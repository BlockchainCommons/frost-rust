use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use bc_components::{
    ARID, Digest, Ed25519PublicKey, JSON, SigningPublicKey, Verifier, XID,
    XIDProvider,
};
use bc_envelope::prelude::*;
use bc_xid::XIDDocument;
use clap::Parser;
use frost_ed25519 as frost;
use gstp::{SealedResponse, SealedResponseBehavior};
use tokio::runtime::Runtime;

use crate::{
    cmd::{
        dkg::common::parse_arid_ur, is_verbose,
        registry::participants_file_path, storage::StorageClient,
    },
    registry::Registry,
};

/// Collect signature shares, aggregate the signature, and post finalize
/// packages (coordinator).
#[derive(Debug, Parser)]
#[group(skip)]
pub struct CommandArgs {
    #[command(flatten)]
    storage: crate::cmd::dkg::OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Wait up to this many seconds for each response to appear
    #[arg(long = "timeout", value_name = "SECONDS")]
    timeout: Option<u64>,

    /// Optional group ID to constrain lookup when multiple groups exist
    #[arg(long = "group", value_name = "UR:ARID")]
    group_id: Option<String>,

    /// Print a sample unsealed finalize package (does not affect sending)
    #[arg(long = "preview-finalize")]
    preview_finalize: bool,

    /// Signing session ID to finalize
    #[arg(value_name = "SESSION_ID")]
    session_id: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let selection = selection
            .context("Hubert storage is required for sign finalize")?;

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
                "Only the coordinator can finalize signing. Coordinator: {}, Owner: {}",
                group_record.coordinator().xid().ur_string(),
                owner.xid().ur_string()
            );
        }

        let commitments_state =
            load_commitments_state(&registry_path, &group_id, &session_id)?;

        if is_verbose() {
            eprintln!(
                "Collecting signature shares for session {} from {} participants...",
                session_id.ur_string(),
                commitments_state.commitments.len()
            );
        }

        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        let mut signature_shares_by_identifier: BTreeMap<
            frost::Identifier,
            frost::round2::SignatureShare,
        > = BTreeMap::new();
        let mut signature_shares_by_xid: BTreeMap<
            XID,
            frost::round2::SignatureShare,
        > = BTreeMap::new();
        let mut finalize_arids: HashMap<XID, ARID> = HashMap::new();

        let xid_to_identifier = xid_identifier_map(&start_state.participants)?;

        for (xid, entry) in &commitments_state.commitments {
            let participant_name = registry
                .participant(xid)
                .and_then(|r| r.pet_name().map(|s| s.to_owned()))
                .unwrap_or_else(|| xid.ur_string());
            if is_verbose() {
                eprintln!("{participant_name}...");
            }

            let identifier = xid_to_identifier
                .get(xid)
                .context("Identifier mapping missing for participant")?;
            let (signature_share, finalize_arid) = fetch_share_response(
                &runtime,
                &client,
                &entry.share_arid,
                self.timeout,
                owner.xid_document(),
                xid,
                &session_id,
            )?;
            signature_shares_by_identifier.insert(*identifier, signature_share);
            signature_shares_by_xid.insert(*xid, signature_share);
            finalize_arids.insert(*xid, finalize_arid);
        }

        if signature_shares_by_identifier.len() < start_state.min_signers {
            bail!(
                "Only collected {} signature shares, need at least {}",
                signature_shares_by_identifier.len(),
                start_state.min_signers
            );
        }

        // Build signing package
        let signing_commitments = commitments_with_identifiers(
            &commitments_state.commitments,
            &xid_to_identifier,
        )?;
        let target_digest: Digest = {
            let start_state_target =
                Envelope::from_ur_string(&start_state.target_ur)
                    .context("Invalid target UR in start state")?;
            start_state_target.subject().digest()
        };
        let signing_package = frost::SigningPackage::new(
            signing_commitments,
            target_digest.data(),
        );

        // Public key package from finalize collection
        let public_key_package =
            load_public_key_package(&registry_path, &group_id)?;
        let verifying_key =
            signing_key_from_verifying(public_key_package.verifying_key())?;

        let signature = frost_ed25519::aggregate(
            &signing_package,
            &signature_shares_by_identifier,
            &public_key_package,
        )
        .context("Failed to aggregate signature shares")?;

        // Verify aggregated signature against target digest before dispatch
        let sig_bytes_vec = signature.serialize()?;
        let sig_array: [u8; 64] =
            sig_bytes_vec.as_slice().try_into().map_err(|_| {
                anyhow::anyhow!("Aggregated signature is not 64 bytes")
            })?;
        let final_signature =
            bc_components::Signature::ed25519_from_data(sig_array);
        let signature_ur = final_signature.ur_string();
        if !verifying_key.verify(&final_signature, target_digest.data()) {
            bail!(
                "Aggregated signature failed verification against target digest"
            );
        }

        // Attach and verify on the target envelope
        let signed_envelope = Envelope::from_ur_string(&start_state.target_ur)?
            .add_assertion(
                bc_envelope::known_values::SIGNED,
                final_signature.clone(),
            );
        signed_envelope
            .verify_signature_from(&verifying_key)
            .context(
                "Aggregated signature did not verify on target envelope",
            )?;
        let signed_envelope_ur = signed_envelope.ur_string();

        persist_final_state(
            &registry_path,
            &group_id,
            &session_id,
            &final_signature,
            &signature_shares_by_xid,
            &finalize_arids,
        )?;

        if is_verbose() {
            eprintln!();
            eprintln!(
                "Aggregated signature for session {} and prepared {} finalize packages.",
                session_id.ur_string(),
                finalize_arids.len()
            );
            eprintln!("Signature verified against target and group key.");
        }

        // Dispatch finalize packages to participants
        let signer_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Coordinator XID document has no signing keys")?;
        let valid_until =
            Date::with_duration_from_now(Duration::from_secs(60 * 60));

        if is_verbose() {
            eprintln!(
                "Dispatching finalize packages to {} participants...",
                finalize_arids.len()
            );
        }

        let mut preview_printed = false;
        for (participant, finalize_arid) in &finalize_arids {
            let recipient_doc: XIDDocument = if *participant == owner.xid() {
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

            let request = build_finalize_request(
                owner.xid_document(),
                &session_id,
                &signature_shares_by_xid,
            )?;

            if self.preview_finalize && !preview_printed {
                let preview = request.to_envelope(
                    Some(valid_until),
                    Some(signer_keys),
                    None,
                )?;
                println!(
                    "# signFinalize preview for {}",
                    participant.ur_string()
                );
                println!("{}", preview.format());
                preview_printed = true;
            }

            let sealed = request.to_envelope_for_recipients(
                Some(valid_until),
                Some(signer_keys),
                &[&recipient_doc],
            )?;

            runtime
                .block_on(async { client.put(finalize_arid, &sealed).await })?;
        }

        // Print the final signature and signed envelope UR after all dispatches
        println!("{signature_ur}");
        println!("{signed_envelope_ur}");

        Ok(())
    }
}

fn fetch_share_response(
    runtime: &Runtime,
    client: &StorageClient,
    response_arid: &ARID,
    timeout: Option<u64>,
    coordinator: &XIDDocument,
    expected_sender: &XID,
    expected_session_id: &ARID,
) -> Result<(frost::round2::SignatureShare, ARID)> {
    let envelope = runtime.block_on(async {
        client
            .get(response_arid, timeout)
            .await?
            .context("Signature share response not found in Hubert storage")
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
        bail!("Participant rejected signShare: {}", reason);
    }

    let result = sealed_response
        .result()
        .context("Response has no result envelope")?;

    let function: String = result.extract_subject()?;
    if function != "signShareResponse" {
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

    let signature_share_json: JSON =
        result.extract_object_for_predicate("signature_share")?;
    let signature_share: frost::round2::SignatureShare =
        serde_json::from_slice(signature_share_json.as_bytes())
            .context("Failed to deserialize signature share")?;

    let finalize_arid: ARID =
        result.extract_object_for_predicate("response_arid")?;

    Ok((signature_share, finalize_arid))
}

fn build_finalize_request(
    sender: &XIDDocument,
    session_id: &ARID,
    signature_shares: &BTreeMap<XID, frost::round2::SignatureShare>,
) -> Result<gstp::SealedRequest> {
    let mut request =
        gstp::SealedRequest::new("signFinalize", *session_id, sender)
            .with_parameter("session", *session_id);

    for (xid, share) in signature_shares {
        let entry = Envelope::new(*xid).add_assertion(
            "share",
            CBOR::from(JSON::from_data(serde_json::to_vec(share)?)),
        );
        request = request.with_parameter("signature_share", entry);
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
            "Multiple signing sessions found; specify --group to disambiguate"
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
    let min_signers: usize = raw
        .get("min_signers")
        .and_then(|v| v.as_u64())
        .context("Missing min_signers in start.json")?
        .try_into()
        .context("min_signers does not fit in usize")?;

    let participants_val = raw
        .get("participants")
        .and_then(|v| v.as_object())
        .context("Missing participants in start.json")?;

    let mut participants = Vec::new();
    for xid_str in participants_val.keys() {
        participants.push(
            XID::from_ur_string(xid_str)
                .context("Invalid participant XID in start.json")?,
        );
    }
    participants.sort();

    let target_ur = get_str("target")?;

    Ok(StartState {
        group_id: *group_id,
        min_signers,
        participants,
        target_ur,
    })
}

struct ParticipantCommitment {
    commitments: frost::round1::SigningCommitments,
    share_arid: ARID,
}

struct CommitmentsState {
    commitments: BTreeMap<XID, ParticipantCommitment>,
}

fn load_commitments_state(
    registry_path: &Path,
    group_id: &ARID,
    session_id: &ARID,
) -> Result<CommitmentsState> {
    let dir = signing_state_dir(registry_path, group_id, session_id);
    let path = dir.join("commitments.json");
    if !path.exists() {
        bail!(
            "Commitments not found at {}. Run `frost sign coordinator collect` first",
            path.display()
        );
    }

    let raw: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(
            &fs::read(&path).with_context(|| {
                format!("Failed to read {}", path.display())
            })?,
        )
        .context("Invalid commitments.json")?;

    let get_str = |key: &str| -> Result<String> {
        raw.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .with_context(|| {
                format!("Missing or invalid {key} in commitments.json")
            })
    };

    let session_in_state = parse_arid_ur(&get_str("session")?)?;
    if session_in_state != *session_id {
        bail!(
            "commitments.json session {} does not match requested session {}",
            session_in_state.ur_string(),
            session_id.ur_string()
        );
    }

    let commitments_val = raw
        .get("commitments")
        .and_then(|v| v.as_object())
        .context("Missing commitments map in commitments.json")?;

    let mut commitments = BTreeMap::new();
    for (xid_str, value) in commitments_val {
        let xid = XID::from_ur_string(xid_str)
            .context("Invalid participant XID in commitments.json")?;
        let obj = value.as_object().context(
            "Participant entry is not an object in commitments.json",
        )?;

        let commit_value = obj
            .get("commitments")
            .cloned()
            .context("Missing commitments value in commitments.json")?;
        let commitments_parsed: frost::round1::SigningCommitments =
            serde_json::from_value(commit_value)
                .context("Failed to parse SigningCommitments")?;

        let share_arid_raw = obj
            .get("share_arid")
            .and_then(|v| v.as_str())
            .context("Missing share_arid in commitments.json")?;
        let share_arid = parse_arid_ur(share_arid_raw)?;

        commitments.insert(
            xid,
            ParticipantCommitment {
                commitments: commitments_parsed,
                share_arid,
            },
        );
    }

    Ok(CommitmentsState { commitments })
}

fn load_public_key_package(
    registry_path: &Path,
    group_id: &ARID,
) -> Result<frost_ed25519::keys::PublicKeyPackage> {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let path = base
        .join("group-state")
        .join(group_id.hex())
        .join("collected_finalize.json");
    if !path.exists() {
        bail!(
            "collected_finalize.json not found at {}. Run `frost dkg coordinator finalize collect` first",
            path.display()
        );
    }

    let raw: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(
            &fs::read(&path).with_context(|| {
                format!("Failed to read {}", path.display())
            })?,
        )
        .context("Invalid collected_finalize.json")?;

    let first_entry = raw
        .values()
        .next()
        .context("collected_finalize.json is empty")?;
    let public_key_value = first_entry
        .as_object()
        .and_then(|m| m.get("public_key_package"))
        .cloned()
        .context("public_key_package missing in collected_finalize.json")?;

    let pkg: frost_ed25519::keys::PublicKeyPackage =
        serde_json::from_value(public_key_value)
            .context("Failed to parse public_key_package")?;

    Ok(pkg)
}

fn signing_key_from_verifying(
    verifying_key: &frost_ed25519::VerifyingKey,
) -> Result<SigningPublicKey> {
    let bytes = verifying_key
        .serialize()
        .map_err(|e| anyhow!("Failed to serialize verifying key: {e}"))?;
    let ed25519 = Ed25519PublicKey::from_data_ref(bytes)
        .context("Group verifying key is not a valid Ed25519 public key")?;
    Ok(SigningPublicKey::from_ed25519(ed25519))
}

fn commitments_with_identifiers(
    commitments: &BTreeMap<XID, ParticipantCommitment>,
    xid_to_identifier: &HashMap<XID, frost::Identifier>,
) -> Result<BTreeMap<frost::Identifier, frost::round1::SigningCommitments>> {
    let mut mapped = BTreeMap::new();
    for (xid, entry) in commitments {
        let identifier = xid_to_identifier.get(xid).ok_or_else(|| {
            anyhow::anyhow!("Unknown participant {}", xid.ur_string())
        })?;
        mapped.insert(*identifier, entry.commitments);
    }
    Ok(mapped)
}

fn xid_identifier_map(
    participants: &[XID],
) -> Result<HashMap<XID, frost::Identifier>> {
    let mut map = HashMap::new();
    for (i, xid) in participants.iter().enumerate() {
        let identifier = frost::Identifier::try_from((i + 1) as u16)
            .context("Failed to derive Identifier from participant index")?;
        map.insert(*xid, identifier);
    }
    Ok(map)
}

fn persist_final_state(
    registry_path: &Path,
    group_id: &ARID,
    session_id: &ARID,
    signature: &bc_components::Signature,
    signature_shares: &BTreeMap<XID, frost::round2::SignatureShare>,
    finalize_arids: &HashMap<XID, ARID>,
) -> Result<()> {
    let dir = signing_state_dir(registry_path, group_id, session_id);
    fs::create_dir_all(&dir).with_context(|| {
        format!("Failed to create signing state directory {}", dir.display())
    })?;

    let mut shares_json = serde_json::Map::new();
    for (xid, share) in signature_shares {
        shares_json.insert(
            xid.ur_string(),
            serde_json::to_value(share)
                .context("Failed to serialize signature share")?,
        );
    }

    let mut finalize_json = serde_json::Map::new();
    for (xid, arid) in finalize_arids {
        finalize_json.insert(
            xid.ur_string(),
            serde_json::Value::String(arid.ur_string()),
        );
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
        "signature".to_string(),
        serde_json::Value::String(signature.ur_string()),
    );
    root.insert(
        "signature_shares".to_string(),
        serde_json::Value::Object(shares_json),
    );
    root.insert(
        "finalize_arids".to_string(),
        serde_json::Value::Object(finalize_json),
    );

    fs::write(dir.join("final.json"), serde_json::to_vec_pretty(&root)?)
        .with_context(|| {
            format!("Failed to write {}", dir.join("final.json").display())
        })
}

fn signing_state_dir(
    registry_path: &Path,
    group_id: &ARID,
    session_id: &ARID,
) -> PathBuf {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("group-state")
        .join(group_id.hex())
        .join("signing")
        .join(session_id.hex())
}

struct StartState {
    group_id: ARID,
    min_signers: usize,
    participants: Vec<XID>,
    target_ur: String,
}
