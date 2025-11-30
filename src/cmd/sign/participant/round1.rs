use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, JSON, XID};
use bc_envelope::prelude::*;
use clap::Parser;
use frost_ed25519::{self as frost};
use gstp::{
    SealedRequest, SealedRequestBehavior, SealedResponse,
    SealedResponseBehavior,
};
use rand_core::OsRng;
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

/// Respond to a signInvite request (participant).
#[derive(Debug, Parser)]
#[group(skip)]
pub struct CommandArgs {
    #[command(flatten)]
    storage: OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Print the preview response envelope UR instead of sending
    #[arg(long = "preview")]
    preview: bool,

    /// Reject the signInvite request with the provided reason
    #[arg(long = "reject", value_name = "REASON")]
    reject_reason: Option<String>,

    /// Optional group ID hint when multiple groups exist
    #[arg(long = "group", value_name = "UR:ARID")]
    group_id: Option<String>,

    /// Signing session ID to respond to
    #[arg(value_name = "SESSION_ID")]
    session: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        if selection.is_none() && !self.preview {
            bail!("Hubert storage is required for sign commit");
        }
        if selection.is_some() && self.preview {
            bail!("--preview cannot be used with Hubert storage options");
        }

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

        let receive_state = load_receive_state(
            &registry_path,
            &session_id,
            group_hint,
            &registry,
        )?;
        let group_id = receive_state.group_id;
        let group_record = registry
            .group(&group_id)
            .context("Group not found in registry")?
            .clone();

        // Decrypt persisted request to validate and get peer continuation
        let owner_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Owner XID document has no private keys")?;
        let sealed_request = SealedRequest::try_from_envelope(
            &receive_state.request_envelope,
            None,
            Some(Date::now()),
            owner_keys,
        )?;

        if sealed_request.function() != &Function::from("signInvite") {
            bail!("Unexpected request function: {}", sealed_request.function());
        }

        if sealed_request.id() != session_id {
            bail!(
                "Session ID mismatch (state {}, request {})",
                session_id.ur_string(),
                sealed_request.id().ur_string()
            );
        }

        let request_group: ARID =
            sealed_request.extract_object_for_parameter("group")?;
        if request_group != group_id {
            bail!(
                "Group ID mismatch (state {}, request {})",
                group_id.ur_string(),
                request_group.ur_string()
            );
        }

        if !receive_state.participants.contains(&owner.xid()) {
            bail!(
                "Persisted signInvite request does not include this participant"
            );
        }

        // Load key package
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

        let target_envelope =
            Envelope::from_ur_string(&receive_state.target_ur)
                .context("Invalid target UR in persisted state")?;

        // Reject path
        let next_share_arid = if self.reject_reason.is_none() {
            Some(ARID::new())
        } else {
            None
        };

        let signer_private_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Owner XID document has no signing keys")?;

        let sealed_response = if let Some(reason) = self.reject_reason.clone() {
            let error_body = Envelope::new("signCommitReject")
                .add_assertion("group", group_id)
                .add_assertion("session", session_id)
                .add_assertion("reason", reason.clone());

            SealedResponse::new_failure(
                sealed_request.id(),
                owner.xid_document().clone(),
            )
            .with_error(error_body)
            .with_peer_continuation(sealed_request.peer_continuation())
        } else {
            // Run signing part1
            let (signing_nonces, signing_commitments) =
                frost::round1::commit(key_package.signing_share(), &mut OsRng);

            let commitments_json =
                JSON::from_data(serde_json::to_vec(&signing_commitments)?);

            let next_share =
                next_share_arid.expect("next share ARID present on accept");

            let response_body = Envelope::unit()
                .add_type("signRound1Response")
                .add_assertion("session", session_id)
                .add_assertion("commitments", CBOR::from(commitments_json))
                .add_assertion("response_arid", next_share);

            // Persist part1 state
            if !self.preview {
                persist_commit_state(
                    &registry_path,
                    &group_id,
                    &session_id,
                    &receive_state,
                    &signing_nonces,
                    &signing_commitments,
                    &target_envelope,
                    next_share,
                )?;
                // Update listening ARID for next request
                let group_record = registry
                    .group_mut(&group_id)
                    .context("Group not found in registry")?;
                group_record.set_listening_at_arid(next_share);
                registry.save(&registry_path)?;
            }

            SealedResponse::new_success(
                sealed_request.id(),
                owner.xid_document().clone(),
            )
            .with_result(response_body)
            .with_peer_continuation(sealed_request.peer_continuation())
        };

        if self.preview {
            let unsealed = sealed_response.to_envelope(
                None,
                Some(signer_private_keys),
                None,
            )?;
            println!("{}", unsealed.ur_string());
            return Ok(());
        }

        let response_envelope = sealed_response.to_envelope(
            Some(Date::with_duration_from_now(Duration::from_secs(60 * 60))),
            Some(signer_private_keys),
            Some(&receive_state.coordinator_doc),
        )?;

        let selection =
            selection.context("Hubert storage is required for sign commit")?;
        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        if is_verbose() {
            eprintln!(
                "Posting signInvite response to {}",
                receive_state.response_arid.ur_string()
            );
        }

        runtime.block_on(async {
            client
                .put(&receive_state.response_arid, &response_envelope)
                .await
        })?;

        // On reject, clear listening ARID
        if self.reject_reason.is_some() {
            let group_record = registry
                .group_mut(&group_id)
                .context("Group not found in registry")?;
            group_record.clear_listening_at_arid();
            registry.save(&registry_path)?;
        }

        Ok(())
    }
}

struct ReceiveState {
    group_id: ARID,
    coordinator_doc: bc_xid::XIDDocument,
    response_arid: ARID,
    target_ur: String,
    participants: Vec<XID>,
    request_envelope: Envelope,
}

fn load_receive_state(
    registry_path: &Path,
    session_id: &ARID,
    group_hint: Option<ARID>,
    registry: &Registry,
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
    let response_arid = parse_arid_ur(&get_str("response_arid")?)?;
    let target_ur = get_str("target")?;
    let coordinator_ur = get_str("coordinator")?;
    let coordinator_xid = XID::from_ur_string(&coordinator_ur)
        .context("Invalid coordinator XID in sign_receive.json")?;
    let coordinator_doc = if let Some(record) =
        registry.participant(&coordinator_xid)
    {
        record.xid_document().clone()
    } else if let Some(owner) = registry.owner()
        && owner.xid() == coordinator_xid
    {
        owner.xid_document().clone()
    } else {
        bail!(
            "Coordinator {} not found in registry and cannot resolve encryption key",
            coordinator_xid.ur_string()
        );
    };
    let request_env_ur = get_str("request_envelope")?;
    let request_envelope = Envelope::from_ur_string(&request_env_ur)
        .context("Invalid request_envelope UR")?;

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

    Ok(ReceiveState {
        group_id: *group_id,
        coordinator_doc,
        response_arid,
        target_ur,
        participants,
        request_envelope,
    })
}

#[allow(clippy::too_many_arguments)]
fn persist_commit_state(
    registry_path: &Path,
    group_id: &ARID,
    session_id: &ARID,
    receive_state: &ReceiveState,
    signing_nonces: &frost::round1::SigningNonces,
    signing_commitments: &frost::round1::SigningCommitments,
    target_envelope: &Envelope,
    next_share_arid: ARID,
) -> Result<()> {
    let dir = signing_state_dir(registry_path, group_id, session_id);
    fs::create_dir_all(&dir).with_context(|| {
        format!("Failed to create signing state directory {}", dir.display())
    })?;

    let mut root = serde_json::Map::new();
    root.insert(
        "session".to_string(),
        serde_json::Value::String(session_id.ur_string()),
    );
    root.insert(
        "response_arid".to_string(),
        serde_json::Value::String(receive_state.response_arid.ur_string()),
    );
    root.insert(
        "next_share_arid".to_string(),
        serde_json::Value::String(next_share_arid.ur_string()),
    );
    root.insert(
        "target".to_string(),
        serde_json::Value::String(target_envelope.ur_string()),
    );
    root.insert(
        "signing_nonces".to_string(),
        serde_json::to_value(signing_nonces)
            .context("Failed to serialize signing nonces")?,
    );
    root.insert(
        "signing_commitments".to_string(),
        serde_json::to_value(signing_commitments)
            .context("Failed to serialize signing commitments")?,
    );

    fs::write(dir.join("commit.json"), serde_json::to_vec_pretty(&root)?)
        .with_context(|| {
            format!("Failed to write {}", dir.join("commit.json").display())
        })
}
