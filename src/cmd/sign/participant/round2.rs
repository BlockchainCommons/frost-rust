use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, Digest, JSON, XID, XIDProvider};
use bc_envelope::prelude::*;
use clap::Parser;
use frost_ed25519 as frost;
use gstp::{
    SealedRequest, SealedRequestBehavior, SealedResponse,
    SealedResponseBehavior,
};
use tokio::runtime::Runtime;

use crate::{
    cmd::{
        dkg::{OptionalStorageSelector, common::parse_arid_ur},
        is_verbose,
        registry::participants_file_path,
        sign::common::signing_state_dir,
        storage::StorageClient,
    },
    registry::Registry,
};

/// Respond to a signRound2 request (participant).
#[derive(Debug, Parser)]
#[group(skip)]
pub struct CommandArgs {
    #[command(flatten)]
    storage: OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Wait up to this many seconds for the request to appear
    #[arg(long = "timeout", value_name = "SECONDS")]
    timeout: Option<u64>,

    /// Print the unsealed response envelope UR instead of sending
    #[arg(long = "preview")]
    preview: bool,

    /// Optional group ID hint when multiple groups contain this session
    #[arg(long = "group", value_name = "UR:ARID")]
    group_id: Option<String>,

    /// Signing session ID to respond to
    #[arg(value_name = "SESSION_ID")]
    session: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let selection =
            selection.context("Hubert storage is required for sign share")?;

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

        if group_record.min_signers() != receive_state.min_signers {
            bail!(
                "Session min_signers {} does not match registry {}",
                receive_state.min_signers,
                group_record.min_signers()
            );
        }

        let registry_participants: BTreeSet<XID> = group_record
            .participants()
            .iter()
            .map(|p| *p.xid())
            .collect();
        let session_participants: BTreeSet<XID> =
            receive_state.participants.iter().copied().collect();
        if registry_participants != session_participants {
            bail!(
                "Session participants do not match registry group participants"
            );
        }

        // Validate owner participates in this session
        if !receive_state.participants.contains(&owner.xid()) {
            bail!("This participant is not part of the signing session");
        }

        // Ensure registry listening ARID matches persisted commit state
        let listening_at_arid = group_record.listening_at_arid().context(
            "No listening ARID for signRound2. Did you run `frost sign participant commit`?",
        )?;

        let commit_state =
            load_commit_state(&registry_path, &group_id, &session_id)?;
        if commit_state.next_share_arid != listening_at_arid {
            bail!(
                "Listening ARID in registry ({}) does not match persisted commit state ({})",
                listening_at_arid.ur_string(),
                commit_state.next_share_arid.ur_string()
            );
        }

        if commit_state.target_ur != receive_state.target_ur {
            bail!(
                "Target envelope in commit state does not match persisted signInvite request"
            );
        }

        let key_package_path = group_record
            .contributions()
            .key_package
            .as_ref()
            .context("Key package path not found; did you finish DKG?")?;
        let key_package: frost::keys::KeyPackage =
            serde_json::from_slice(&fs::read(key_package_path).with_context(
                || format!("Failed to read {}", key_package_path),
            )?)
            .context("Failed to parse key_package.json")?;

        let finalize_arid = ARID::new();

        // Compute target digest from persisted target envelope
        let target_envelope =
            Envelope::from_ur_string(&receive_state.target_ur).with_context(
                || "Invalid target envelope UR in persisted state".to_string(),
            )?;
        let target_digest: Digest = target_envelope.subject().digest();

        if is_verbose() {
            eprintln!("Fetching signRound2 request from Hubert...");
        }

        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        let request_envelope = runtime.block_on(async {
            client
                .get(&listening_at_arid, self.timeout)
                .await?
                .context("signRound2 request not found in Hubert storage")
        })?;

        let signer_private_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Owner XID document has no private keys")?;

        let now = Date::now();
        let sealed_request = SealedRequest::try_from_envelope(
            &request_envelope,
            None,
            Some(now),
            signer_private_keys,
        )?;

        if sealed_request.function() != &Function::from("signRound2") {
            bail!("Unexpected request function: {}", sealed_request.function());
        }

        if sealed_request.id() != session_id {
            bail!(
                "Session ID mismatch (request {}, expected {})",
                sealed_request.id().ur_string(),
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

        let response_arid: ARID =
            sealed_request.extract_object_for_parameter("response_arid")?;

        // Extract commitments from request
        let commitments_by_xid =
            parse_commitments(&sealed_request, &receive_state)?;

        let my_commitments = commitments_by_xid.get(&owner.xid()).context(
            "signRound2 request missing commitments for this participant",
        )?;
        if *my_commitments != commit_state.signing_commitments {
            bail!(
                "signRound2 request commitments do not match locally stored commitments"
            );
        }

        // Map XIDs to identifiers (sorted participant order)
        let xid_to_identifier =
            xid_identifier_map(&receive_state.participants)?;

        let my_identifier = xid_to_identifier
            .get(&owner.xid())
            .context("Identifier for participant not found")?;

        if key_package.identifier() != my_identifier {
            bail!(
                "Key package identifier {:?} does not match expected {:?}",
                key_package.identifier(),
                my_identifier
            );
        }

        if *key_package.min_signers() as usize != receive_state.min_signers {
            bail!(
                "Key package min_signers {} does not match session {}",
                key_package.min_signers(),
                receive_state.min_signers
            );
        }

        if commitments_by_xid.len() < receive_state.min_signers {
            bail!(
                "signRound2 request contained {} commitments but requires at least {} signers",
                commitments_by_xid.len(),
                receive_state.min_signers
            );
        }

        let signing_commitments = commitments_with_identifiers(
            &commitments_by_xid,
            &xid_to_identifier,
        )?;

        let signing_package = frost::SigningPackage::new(
            signing_commitments,
            target_digest.data(),
        );

        let signature_share = frost::round2::sign(
            &signing_package,
            &commit_state.signing_nonces,
            &key_package,
        )
        .map_err(|e| anyhow::anyhow!("FROST signing failed: {}", e))?;

        let response_body = Envelope::unit()
            .add_type("signRound2Response")
            .add_assertion("session", session_id)
            .add_assertion(
                "signature_share",
                CBOR::from(JSON::from_data(serde_json::to_vec(
                    &signature_share,
                )?)),
            )
            .add_assertion("response_arid", finalize_arid);

        let sealed_response = SealedResponse::new_success(
            sealed_request.id(),
            owner.xid_document().clone(),
        )
        .with_result(response_body)
        .with_peer_continuation(sealed_request.peer_continuation());

        if self.preview {
            let unsealed = sealed_response.to_envelope(
                None,
                Some(signer_private_keys),
                None,
            )?;
            println!("{}", unsealed.ur_string());
            return Ok(());
        }

        let coordinator_doc = if *expected_coordinator == owner.xid() {
            owner.xid_document().clone()
        } else {
            registry
                .participant(expected_coordinator)
                .map(|r| r.xid_document().clone())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Coordinator {} not found in registry",
                        expected_coordinator.ur_string()
                    )
                })?
        };

        let response_envelope = sealed_response.to_envelope(
            Some(Date::with_duration_from_now(Duration::from_secs(60 * 60))),
            Some(signer_private_keys),
            Some(&coordinator_doc),
        )?;

        runtime.block_on(async {
            client.put(&response_arid, &response_envelope).await
        })?;

        persist_share_state(
            &registry_path,
            &group_id,
            &session_id,
            &response_arid,
            &finalize_arid,
            &signature_share,
            &commitments_by_xid,
        )?;

        // Set listening ARID for finalize
        let group_record = registry
            .group_mut(&group_id)
            .context("Group not found in registry")?;
        group_record.set_listening_at_arid(finalize_arid);
        registry.save(&registry_path)?;

        if is_verbose() {
            eprintln!(
                "Posted signature share to {}",
                response_arid.ur_string()
            );
        }

        Ok(())
    }
}

fn parse_commitments(
    request: &SealedRequest,
    receive_state: &ReceiveState,
) -> Result<BTreeMap<XID, frost::round1::SigningCommitments>> {
    let mut commitments = BTreeMap::new();
    for entry in request.objects_for_parameter("commitment") {
        let xid: XID = entry.extract_subject()?;
        let commitments_json: JSON =
            entry.extract_object_for_predicate("commitments")?;
        let signing_commitments: frost::round1::SigningCommitments =
            serde_json::from_slice(commitments_json.as_bytes())
                .context("Failed to deserialize commitments")?;
        if commitments.insert(xid, signing_commitments).is_some() {
            bail!("Duplicate commitments for participant {}", xid.ur_string());
        }
    }

    if commitments.is_empty() {
        bail!("signRound2 request contains no commitments");
    }

    // Validate expected participant set
    let expected: BTreeSet<XID> =
        receive_state.participants.iter().copied().collect();
    let actual: BTreeSet<XID> = commitments.keys().copied().collect();
    if expected != actual {
        let missing: Vec<String> = expected
            .difference(&actual)
            .map(|x| x.ur_string())
            .collect();
        let extra: Vec<String> = actual
            .difference(&expected)
            .map(|x| x.ur_string())
            .collect();
        if !missing.is_empty() || !extra.is_empty() {
            bail!(
                "signRound2 commitments do not match session participants (missing: {}; extra: {})",
                missing.join(", "),
                extra.join(", ")
            );
        }
    }

    Ok(commitments)
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

fn persist_share_state(
    registry_path: &Path,
    group_id: &ARID,
    session_id: &ARID,
    response_arid: &ARID,
    finalize_arid: &ARID,
    signature_share: &frost::round2::SignatureShare,
    commitments: &BTreeMap<XID, frost::round1::SigningCommitments>,
) -> Result<()> {
    let dir = signing_state_dir(registry_path, group_id, session_id);
    fs::create_dir_all(&dir).with_context(|| {
        format!("Failed to create signing state directory {}", dir.display())
    })?;

    let mut commitments_json = serde_json::Map::new();
    for (xid, commits) in commitments {
        commitments_json.insert(
            xid.ur_string(),
            serde_json::to_value(commits)
                .context("Failed to serialize commitments")?,
        );
    }

    let mut root = serde_json::Map::new();
    root.insert(
        "session".to_string(),
        serde_json::Value::String(session_id.ur_string()),
    );
    root.insert(
        "response_arid".to_string(),
        serde_json::Value::String(response_arid.ur_string()),
    );
    root.insert(
        "finalize_arid".to_string(),
        serde_json::Value::String(finalize_arid.ur_string()),
    );
    root.insert(
        "finalize_arid".to_string(),
        serde_json::Value::String(finalize_arid.ur_string()),
    );
    root.insert(
        "signature_share".to_string(),
        serde_json::to_value(signature_share)
            .context("Failed to serialize signature share")?,
    );
    root.insert(
        "commitments".to_string(),
        serde_json::Value::Object(commitments_json),
    );

    fs::write(dir.join("share.json"), serde_json::to_vec_pretty(&root)?)
        .with_context(|| {
            format!("Failed to write {}", dir.join("share.json").display())
        })
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

    Ok(ReceiveState {
        group_id: *group_id,
        participants,
        min_signers,
        target_ur,
    })
}

fn load_commit_state(
    registry_path: &Path,
    group_id: &ARID,
    session_id: &ARID,
) -> Result<CommitState> {
    let dir = signing_state_dir(registry_path, group_id, session_id);
    let path = dir.join("commit.json");
    if !path.exists() {
        bail!(
            "Commit state not found at {}. Run `frost sign participant commit` first.",
            path.display()
        );
    }

    let raw: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(
            &fs::read(&path).with_context(|| {
                format!("Failed to read {}", path.display())
            })?,
        )
        .context("Invalid commit.json")?;

    let get_str = |key: &str| -> Result<String> {
        raw.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .with_context(|| format!("Missing or invalid {key} in commit.json"))
    };

    let session_in_state = parse_arid_ur(&get_str("session")?)?;
    if session_in_state != *session_id {
        bail!(
            "Session {} in commit.json does not match requested session {}",
            session_in_state.ur_string(),
            session_id.ur_string()
        );
    }

    let next_share_arid = parse_arid_ur(&get_str("next_share_arid")?)?;
    let target_ur = get_str("target")?;

    let signing_nonces: frost::round1::SigningNonces = serde_json::from_value(
        raw.get("signing_nonces")
            .cloned()
            .context("Missing signing_nonces in commit.json")?,
    )
    .context("Failed to deserialize signing_nonces")?;

    let signing_commitments: frost::round1::SigningCommitments =
        serde_json::from_value(
            raw.get("signing_commitments")
                .cloned()
                .context("Missing signing_commitments in commit.json")?,
        )
        .context("Failed to deserialize signing_commitments")?;

    Ok(CommitState {
        next_share_arid,
        target_ur,
        signing_nonces,
        signing_commitments,
    })
}

struct ReceiveState {
    group_id: ARID,
    participants: Vec<XID>,
    min_signers: usize,
    target_ur: String,
}

struct CommitState {
    next_share_arid: ARID,
    target_ur: String,
    signing_nonces: frost::round1::SigningNonces,
    signing_commitments: frost::round1::SigningCommitments,
}
