use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use bc_components::{
    ARID, Digest, Ed25519PublicKey, JSON, SigningPublicKey, Verifier, XID,
    XIDProvider,
};
use bc_envelope::prelude::*;
use clap::Parser;
use frost_ed25519 as frost;
use gstp::{SealedRequest, SealedRequestBehavior};
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

/// Attach the finalized group signature to the target (participant).
#[derive(Debug, Parser)]
#[group(skip)]
pub struct CommandArgs {
    #[command(flatten)]
    storage: OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Wait up to this many seconds for the finalize package to appear
    #[arg(long = "timeout", value_name = "SECONDS")]
    timeout: Option<u64>,

    /// Optional group ID hint when multiple groups contain this session
    #[arg(long = "group", value_name = "UR:ARID")]
    group_id: Option<String>,

    /// Signing session ID to attach
    #[arg(value_name = "SESSION_ID")]
    session: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let selection =
            selection.context("Hubert storage is required for sign attach")?;

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

        let session_id = parse_arid_ur(&self.session)?;
        let group_hint = match &self.group_id {
            Some(raw) => Some(parse_arid_ur(raw)?),
            None => None,
        };

        let receive_state =
            load_receive_state(&registry_path, &session_id, group_hint)?;
        let group_id = receive_state.group_id;
        let group_record = registry
            .group(&group_id)
            .context("Group not found in registry")?
            .clone();

        if receive_state.coordinator != *group_record.coordinator().xid() {
            bail!("Coordinator in session state does not match registry");
        }

        if !receive_state.participants.contains(&owner.xid()) {
            bail!("This participant is not part of the signing session");
        }

        if group_record.min_signers() != receive_state.min_signers {
            bail!(
                "Session min_signers {} does not match registry {}",
                receive_state.min_signers,
                group_record.min_signers()
            );
        }

        let listening_at_arid = group_record.listening_at_arid().context(
            "No listening ARID for signFinalize. Did you run `frost sign participant share`?",
        )?;

        let share_state =
            load_share_state(&registry_path, &group_id, &session_id)?;
        if share_state.finalize_arid != listening_at_arid {
            bail!(
                "Registry listening ARID ({}) does not match persisted finalize ARID ({})",
                listening_at_arid.ur_string(),
                share_state.finalize_arid.ur_string()
            );
        }

        let commit_participants: BTreeSet<XID> =
            share_state.commitments.keys().copied().collect();
        let session_participants: BTreeSet<XID> =
            receive_state.participants.iter().copied().collect();
        if commit_participants != session_participants {
            bail!("Commitments do not match session participants");
        }

        // Load target envelope and digest from persisted state
        let target_envelope =
            Envelope::from_ur_string(&receive_state.target_ur).with_context(
                || "Invalid target envelope UR in persisted state".to_string(),
            )?;
        let target_digest: Digest = target_envelope.subject().digest();

        if is_verbose() {
            eprintln!("Fetching finalize package from Hubert...");
        }

        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        let finalize_envelope = runtime.block_on(async {
            client
                .get(&share_state.finalize_arid, self.timeout)
                .await?
                .context("Finalize package not found in Hubert storage")
        })?;

        let signer_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Owner XID document has no inception private keys")?;

        let now = Date::now();
        let sealed_request = SealedRequest::try_from_envelope(
            &finalize_envelope,
            None,
            Some(now),
            signer_keys,
        )?;

        if sealed_request.function() != &Function::from("signFinalize") {
            bail!("Unexpected request function: {}", sealed_request.function());
        }

        if sealed_request.id() != session_id {
            bail!(
                "Session ID mismatch (request {}, expected {})",
                sealed_request.id().ur_string(),
                session_id.ur_string()
            );
        }

        let request_session: ARID =
            sealed_request.extract_object_for_parameter("session")?;
        if request_session != session_id {
            bail!(
                "Request session {} does not match expected {}",
                request_session.ur_string(),
                session_id.ur_string()
            );
        }

        // Validate sender (coordinator)
        let expected_coordinator = group_record.coordinator().xid();
        if sealed_request.sender().xid() != *expected_coordinator {
            bail!(
                "Unexpected request sender: {} (expected coordinator {})",
                sealed_request.sender().xid().ur_string(),
                expected_coordinator.ur_string()
            );
        }

        // Extract and validate signature shares
        let signature_shares_by_xid = parse_signature_shares(&sealed_request)?;

        if signature_shares_by_xid.len() < receive_state.min_signers {
            bail!(
                "Finalize package contains {} signature shares but requires at least {}",
                signature_shares_by_xid.len(),
                receive_state.min_signers
            );
        }

        let shares_participants: BTreeSet<XID> =
            signature_shares_by_xid.keys().copied().collect();
        if shares_participants != session_participants {
            bail!("Signature share set does not match session participants");
        }

        if let Some(my_share) = signature_shares_by_xid.get(&owner.xid()) {
            if my_share != &share_state.signature_share {
                bail!(
                    "Finalize package contains a signature share for this participant that does not match local state"
                );
            }
        } else {
            bail!(
                "Finalize package is missing this participant's signature share"
            );
        }

        // Build identifier mapping and signing package
        let xid_to_identifier =
            xid_identifier_map(&receive_state.participants)?;
        let signing_commitments = commitments_with_identifiers(
            &share_state.commitments,
            &xid_to_identifier,
        )?;
        let signing_package = frost::SigningPackage::new(
            signing_commitments,
            target_digest.data(),
        );

        let signature_shares_by_identifier = signature_shares_with_identifiers(
            &signature_shares_by_xid,
            &xid_to_identifier,
        )?;

        let public_key_package =
            load_public_key_package(&registry_path, &group_id)?;
        let verifying_key =
            signing_key_from_verifying(public_key_package.verifying_key())?;

        if let Some(existing) = group_record.verifying_key() {
            if existing != &verifying_key {
                bail!("Registry verifying key does not match finalize package");
            }
        } else {
            let group_record = registry
                .group_mut(&group_id)
                .context("Group not found in registry")?;
            group_record.set_verifying_key(verifying_key.clone());
            registry.save(&registry_path)?;
        }

        let aggregated_signature = frost_ed25519::aggregate(
            &signing_package,
            &signature_shares_by_identifier,
            &public_key_package,
        )
        .context("Failed to aggregate signature shares")?;

        let sig_bytes_vec = aggregated_signature.serialize()?;
        let sig_array: [u8; 64] =
            sig_bytes_vec.as_slice().try_into().map_err(|_| {
                anyhow::anyhow!("Aggregated signature is not 64 bytes")
            })?;
        let final_signature =
            bc_components::Signature::ed25519_from_data(sig_array);

        if !verifying_key.verify(&final_signature, target_digest.data()) {
            bail!(
                "Aggregated signature failed verification against target digest"
            );
        }

        let signed_envelope = target_envelope.clone().add_assertion(
            bc_envelope::known_values::SIGNED,
            final_signature.clone(),
        );
        signed_envelope
            .verify_signature_from(&verifying_key)
            .context(
                "Aggregated signature did not verify on target envelope",
            )?;

        persist_final_state(
            &registry_path,
            &group_id,
            &session_id,
            &final_signature,
            &signed_envelope,
            &signature_shares_by_xid,
            &share_state,
        )?;

        // Clear listening ARID now that the finalize package has been consumed
        let group_record = registry
            .group_mut(&group_id)
            .context("Group not found in registry")?;
        group_record.clear_listening_at_arid();
        registry.save(&registry_path)?;

        println!("{}", final_signature.ur_string());
        println!("{}", signed_envelope.ur_string());

        Ok(())
    }
}

fn parse_signature_shares(
    request: &SealedRequest,
) -> Result<BTreeMap<XID, frost::round2::SignatureShare>> {
    let mut shares = BTreeMap::new();
    for entry in request.objects_for_parameter("signature_share") {
        let xid: XID = entry.extract_subject()?;
        let share_json: JSON = entry.extract_object_for_predicate("share")?;
        let share: frost::round2::SignatureShare =
            serde_json::from_slice(share_json.as_bytes())
                .context("Failed to deserialize signature share")?;
        if shares.insert(xid, share).is_some() {
            bail!(
                "Duplicate signature share for participant {}",
                xid.ur_string()
            );
        }
    }

    if shares.is_empty() {
        bail!("Finalize package contains no signature shares");
    }

    Ok(shares)
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

fn commitments_with_identifiers(
    commitments: &BTreeMap<XID, frost::round1::SigningCommitments>,
    xid_to_identifier: &HashMap<XID, frost::Identifier>,
) -> Result<BTreeMap<frost::Identifier, frost::round1::SigningCommitments>> {
    let mut mapped = BTreeMap::new();
    for (xid, commits) in commitments {
        let identifier = xid_to_identifier.get(xid).ok_or_else(|| {
            anyhow::anyhow!("Unknown participant {}", xid.ur_string())
        })?;
        mapped.insert(*identifier, *commits);
    }
    Ok(mapped)
}

fn signature_shares_with_identifiers(
    shares: &BTreeMap<XID, frost::round2::SignatureShare>,
    xid_to_identifier: &HashMap<XID, frost::Identifier>,
) -> Result<BTreeMap<frost::Identifier, frost::round2::SignatureShare>> {
    let mut mapped = BTreeMap::new();
    for (xid, share) in shares {
        let identifier = xid_to_identifier.get(xid).ok_or_else(|| {
            anyhow::anyhow!("Unknown participant {}", xid.ur_string())
        })?;
        mapped.insert(*identifier, *share);
    }
    Ok(mapped)
}

fn load_receive_state(
    registry_path: &Path,
    session_id: &ARID,
    group_hint: Option<ARID>,
) -> Result<ReceiveState> {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let group_state_dir = base.join("group-state");

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

    let mut candidates = Vec::new();
    for (group_id, group_dir) in group_dirs {
        let candidate = group_dir
            .join("signing")
            .join(session_id.hex())
            .join("sign_receive.json");
        if candidate.exists() {
            candidates.push((group_id, candidate));
        }
    }

    if candidates.is_empty() {
        bail!(
            "No sign_receive.json found for this session; run `frost sign participant receive` first"
        );
    }
    if candidates.len() > 1 {
        bail!(
            "Multiple groups contain this session; use --group to disambiguate"
        );
    }

    let (group_id, path) = &candidates[0];
    let raw: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(
            &fs::read(path).with_context(|| {
                format!("Failed to read {}", path.display())
            })?,
        )
        .context("Invalid sign_receive.json")?;

    let get_str = |key: &str| -> Result<String> {
        raw.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .with_context(|| {
                format!("Missing or invalid {key} in sign_receive.json")
            })
    };

    let session_str = get_str("session")?;
    let session_in_state = parse_arid_ur(&session_str)?;
    if session_in_state != *session_id {
        bail!(
            "Session {} in sign_receive.json does not match requested session {}",
            session_in_state.ur_string(),
            session_id.ur_string()
        );
    }

    let group_in_state = parse_arid_ur(&get_str("group")?)?;
    if group_in_state != *group_id {
        bail!(
            "Group {} in sign_receive.json does not match directory group {}",
            group_in_state.ur_string(),
            group_id.ur_string()
        );
    }

    let coordinator = XID::from_ur_string(&get_str("coordinator")?)
        .context("Invalid coordinator XID in sign_receive.json")?;

    let participants_val =
        raw.get("participants")
            .and_then(|v| v.as_array())
            .context("Missing participants in sign_receive.json")?;
    let mut participants: Vec<XID> = Vec::new();
    for entry in participants_val {
        let s = entry
            .as_str()
            .context("Invalid participant entry in sign_receive.json")?;
        participants.push(XID::from_ur_string(s)?);
    }

    let min_signers: usize = raw
        .get("min_signers")
        .and_then(|v| v.as_u64())
        .context("Missing min_signers in sign_receive.json")?
        .try_into()
        .context("min_signers does not fit in usize")?;

    let target_ur = get_str("target")?;

    participants.sort();

    Ok(ReceiveState {
        group_id: *group_id,
        coordinator,
        participants,
        min_signers,
        target_ur,
    })
}

fn load_share_state(
    registry_path: &Path,
    group_id: &ARID,
    session_id: &ARID,
) -> Result<ShareState> {
    let dir = signing_state_dir(registry_path, group_id, session_id);
    let path = dir.join("share.json");
    if !path.exists() {
        bail!(
            "Signature share state not found at {}. Run `frost sign participant share` first.",
            path.display()
        );
    }

    let raw: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(
            &fs::read(&path).with_context(|| {
                format!("Failed to read {}", path.display())
            })?,
        )
        .context("Invalid share.json")?;

    let get_str = |key: &str| -> Result<String> {
        raw.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .with_context(|| format!("Missing or invalid {key} in share.json"))
    };

    let session_in_state = parse_arid_ur(&get_str("session")?)?;
    if session_in_state != *session_id {
        bail!(
            "Session {} in share.json does not match requested session {}",
            session_in_state.ur_string(),
            session_id.ur_string()
        );
    }

    let finalize_arid = parse_arid_ur(&get_str("finalize_arid")?)?;

    let signature_share: frost::round2::SignatureShare =
        serde_json::from_value(
            raw.get("signature_share")
                .cloned()
                .context("Missing signature_share in share.json")?,
        )
        .context("Failed to deserialize signature_share")?;

    let commitments_val = raw
        .get("commitments")
        .and_then(|v| v.as_object())
        .context("Missing commitments map in share.json")?;

    let mut commitments = BTreeMap::new();
    for (xid_str, value) in commitments_val {
        let xid = XID::from_ur_string(xid_str)
            .context("Invalid participant XID in share.json")?;
        let commits: frost::round1::SigningCommitments =
            serde_json::from_value(value.clone())
                .context("Failed to parse SigningCommitments")?;
        commitments.insert(xid, commits);
    }

    Ok(ShareState { finalize_arid, signature_share, commitments })
}

fn load_public_key_package(
    registry_path: &Path,
    group_id: &ARID,
) -> Result<frost_ed25519::keys::PublicKeyPackage> {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let direct_path = base
        .join("group-state")
        .join(group_id.hex())
        .join("public_key_package.json");
    if direct_path.exists() {
        let pkg: frost_ed25519::keys::PublicKeyPackage =
            serde_json::from_slice(&fs::read(&direct_path).with_context(
                || format!("Failed to read {}", direct_path.display()),
            )?)
            .context("Failed to parse public_key_package.json")?;
        return Ok(pkg);
    }

    // Fallback to collected_finalize.json (coordinator)
    let collected_path = base
        .join("group-state")
        .join(group_id.hex())
        .join("collected_finalize.json");
    if collected_path.exists() {
        let raw: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&fs::read(&collected_path).with_context(
                || format!("Failed to read {}", collected_path.display()),
            )?)
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
        return Ok(pkg);
    }

    bail!(
        "Public key package not found for group {}; run finalize respond/collect first",
        group_id.ur_string()
    );
}

fn signing_key_from_verifying(
    verifying_key: &frost_ed25519::VerifyingKey,
) -> Result<SigningPublicKey> {
    let bytes = verifying_key.serialize().map_err(|e| {
        anyhow::anyhow!("Failed to serialize verifying key: {e}")
    })?;
    let ed25519 = Ed25519PublicKey::from_data_ref(bytes)
        .context("Group verifying key is not a valid Ed25519 public key")?;
    Ok(SigningPublicKey::from_ed25519(ed25519))
}

fn persist_final_state(
    registry_path: &Path,
    group_id: &ARID,
    session_id: &ARID,
    signature: &bc_components::Signature,
    signed_envelope: &Envelope,
    signature_shares: &BTreeMap<XID, frost::round2::SignatureShare>,
    share_state: &ShareState,
) -> Result<()> {
    let dir = signing_state_dir(registry_path, group_id, session_id);
    fs::create_dir_all(&dir).with_context(|| {
        format!("Failed to create signing state directory {}", dir.display())
    })?;

    let final_path = dir.join("final.json");
    let mut root = if final_path.exists() {
        serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(
            &fs::read(&final_path).with_context(|| {
                format!("Failed to read {}", final_path.display())
            })?,
        )
        .context("Invalid existing final.json")?
    } else {
        serde_json::Map::new()
    };

    let mut shares_json = serde_json::Map::new();
    for (xid, share) in signature_shares {
        shares_json.insert(
            xid.ur_string(),
            serde_json::to_value(share)
                .context("Failed to serialize signature share")?,
        );
    }

    let mut commitments_json = serde_json::Map::new();
    for (xid, commits) in &share_state.commitments {
        commitments_json.insert(
            xid.ur_string(),
            serde_json::to_value(commits)
                .context("Failed to serialize commitments")?,
        );
    }

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
        "commitments".to_string(),
        serde_json::Value::Object(commitments_json),
    );
    root.insert(
        "finalize_arid".to_string(),
        serde_json::Value::String(share_state.finalize_arid.ur_string()),
    );
    root.insert(
        "signed_target".to_string(),
        serde_json::Value::String(signed_envelope.ur_string()),
    );

    fs::write(final_path, serde_json::to_vec_pretty(&root)?).with_context(
        || format!("Failed to write {}", dir.join("final.json").display()),
    )
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

struct ReceiveState {
    group_id: ARID,
    coordinator: XID,
    participants: Vec<XID>,
    min_signers: usize,
    target_ur: String,
}

struct ShareState {
    finalize_arid: ARID,
    signature_share: frost::round2::SignatureShare,
    commitments: BTreeMap<XID, frost::round1::SigningCommitments>,
}
