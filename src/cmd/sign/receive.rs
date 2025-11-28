use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, XID, XIDProvider};
use bc_envelope::prelude::*;
use clap::Parser;
use gstp::{SealedRequest, SealedRequestBehavior};
use tokio::runtime::Runtime;

use crate::{
    cmd::{
        dkg::{
            OptionalStorageSelector,
            common::{
                format_name_with_owner_marker, parse_arid_ur,
                parse_envelope_ur, resolve_sender, resolve_sender_name,
            },
        },
        registry::participants_file_path,
        storage::{StorageClient, StorageSelection},
    },
    registry::Registry,
};

/// Inspect a signCommit request (participant).
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

    /// Suppress printing the request envelope UR
    #[arg(long = "no-envelope")]
    no_envelope: bool,

    /// Show request details (coordinator, participants, ARIDs, target digest)
    #[arg(long)]
    info: bool,

    /// Optionally require the request to come from this sender (ur:xid or pet
    /// name in registry)
    #[arg(long = "sender", value_name = "SENDER")]
    sender: Option<String>,

    /// signCommit request ARID or envelope (ur:arid or ur:envelope)
    #[arg(value_name = "REQUEST")]
    request: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        if selection.is_none() && self.timeout.is_some() {
            bail!("--timeout requires Hubert storage parameters");
        }

        let registry_path = participants_file_path(self.registry.clone())?;
        let registry = Registry::load(&registry_path).with_context(|| {
            format!("Failed to load registry at {}", registry_path.display())
        })?;
        let owner = registry
            .owner()
            .context("Registry owner with private keys is required")?
            .clone();

        let expected_sender = match &self.sender {
            Some(raw) => Some(resolve_sender(&registry, raw)?),
            None => None,
        };

        let envelope = resolve_sign_request(
            selection.clone(),
            &self.request,
            self.timeout,
        )?;

        let now = Date::now();
        let recipient_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Owner XID document has no inception private keys")?;
        let sealed_request = SealedRequest::try_from_envelope(
            &envelope,
            None,
            Some(now),
            recipient_keys,
        )?;

        // Validate sender
        if let Some(expected) = expected_sender.as_ref() {
            if sealed_request.sender().xid() != expected.xid() {
                bail!(
                    "Request sender does not match expected sender (got {}, expected {})",
                    sealed_request.sender().xid().ur_string(),
                    expected.xid().ur_string()
                );
            }
        } else {
            let sender_xid = sealed_request.sender().xid();
            let known_owner = registry
                .owner()
                .map(|o| o.xid() == sender_xid)
                .unwrap_or(false);
            let known_participant = registry.participant(&sender_xid).is_some();
            if !known_owner && !known_participant {
                bail!(
                    "Request sender not found in registry: {}",
                    sender_xid.ur_string()
                );
            }
        }

        // Validate function
        if sealed_request.function() != &Function::from("signCommit") {
            bail!("Unexpected request function: {}", sealed_request.function());
        }

        // Parameters
        let valid_until: Date =
            sealed_request.extract_object_for_parameter("validUntil")?;
        if valid_until <= now {
            bail!("signCommit request has expired");
        }

        let group_id: ARID =
            sealed_request.extract_object_for_parameter("group")?;
        let session_id: ARID =
            sealed_request.extract_object_for_parameter("session")?;
        let min_signers: usize = sealed_request
            .extract_object_for_parameter::<u64>("minSigners")?
            .try_into()
            .context("minSigners does not fit in usize")?;

        // Participants and the recipient's commit ARID
        let mut participants: Vec<XID> = Vec::new();
        let mut response_arid: Option<ARID> = None;
        for entry in sealed_request.objects_for_parameter("participant") {
            let xid: XID = entry.extract_subject()?;
            if xid == owner.xid() {
                let encrypted_arid =
                    entry.object_for_predicate("response_arid")?;
                let arid_env =
                    encrypted_arid.decrypt_to_recipient(recipient_keys)?;
                let arid: ARID = arid_env.extract_subject()?;
                response_arid = Some(arid);
            }
            participants.push(xid);
        }

        if participants.is_empty() {
            bail!("signCommit request contains no participants");
        }
        if min_signers < 2 {
            bail!("minSigners must be at least 2");
        }
        if min_signers > participants.len() {
            bail!("minSigners exceeds participant count");
        }
        if !participants.contains(&owner.xid()) {
            bail!("signCommit request does not include this participant");
        }
        let response_arid = response_arid
            .context("signCommit request missing response ARID")?;

        participants.sort();

        let target_ur_opt: Option<String> =
            sealed_request.extract_optional_object_for_parameter("targetUR")?;
        let target_envelope = if let Some(raw) = target_ur_opt {
            Envelope::from_ur_string(&raw).context("Invalid target UR")?
        } else {
            sealed_request.object_for_parameter("target")?
        };

        if self.info {
            let coordinator_name =
                resolve_sender_name(&registry, sealed_request.sender())
                    .unwrap_or_else(|| {
                        sealed_request.sender().xid().ur_string()
                    });
            let participant_names =
                format_participant_names(&registry, &participants, &owner);
            println!("Group: {}", group_id.ur_string());
            println!("Coordinator: {}", coordinator_name);
            println!("Min signers: {}", min_signers);
            println!("Participants: {}", participant_names.join(", "));
            println!("Target:");
            println!("{}", target_envelope.format());
        }

        // Primary output for scripting: session ID on its own line (no header).
        println!("{}", session_id.ur_string());

        // Persist request details for follow-up commands
        let state_dir =
            signing_state_dir(&registry_path, &group_id, &session_id);
        fs::create_dir_all(&state_dir).with_context(|| {
            format!(
                "Failed to create signing state directory {}",
                state_dir.display()
            )
        })?;

        let mut root = serde_json::Map::new();
        root.insert(
            "request_envelope".to_string(),
            serde_json::Value::String(envelope.ur_string()),
        );
        root.insert(
            "group".to_string(),
            serde_json::Value::String(group_id.ur_string()),
        );
        root.insert(
            "session".to_string(),
            serde_json::Value::String(session_id.ur_string()),
        );
        root.insert(
            "coordinator".to_string(),
            serde_json::Value::String(
                sealed_request.sender().xid().ur_string(),
            ),
        );
        root.insert(
            "min_signers".to_string(),
            serde_json::Value::Number(min_signers.into()),
        );
        root.insert(
            "response_arid".to_string(),
            serde_json::Value::String(response_arid.ur_string()),
        );
        root.insert(
            "participants".to_string(),
            serde_json::Value::Array(
                participants
                    .iter()
                    .map(|xid| serde_json::Value::String(xid.ur_string()))
                    .collect(),
            ),
        );
        root.insert(
            "target".to_string(),
            serde_json::Value::String(target_envelope.ur_string()),
        );
        fs::write(
            state_dir.join("sign_receive.json"),
            serde_json::to_vec_pretty(&root)?,
        )
        .context("Failed to persist signCommit request details")?;

        Ok(())
    }
}

fn resolve_sign_request(
    selection: Option<StorageSelection>,
    request: &str,
    timeout: Option<u64>,
) -> Result<Envelope> {
    if let Some(selection) = selection {
        if let Ok(arid) = parse_arid_ur(request) {
            let runtime = Runtime::new()?;
            return runtime.block_on(async move {
                let client = StorageClient::from_selection(selection).await?;
                client
                    .get(&arid, timeout)
                    .await?
                    .context("signCommit request not found in Hubert storage")
            });
        }
        if timeout.is_some() {
            bail!(
                "--timeout is only valid when retrieving requests from Hubert"
            );
        }
        return parse_envelope_ur(request);
    }

    if parse_arid_ur(request).is_ok() {
        bail!(
            "Hubert storage parameters are required to retrieve requests by ARID"
        );
    }
    parse_envelope_ur(request)
}

fn format_participant_names(
    registry: &Registry,
    participants: &[XID],
    owner: &crate::registry::OwnerRecord,
) -> Vec<String> {
    participants
        .iter()
        .map(|xid| {
            let is_owner = *xid == owner.xid();
            let name = if is_owner {
                owner
                    .pet_name()
                    .map(|n| n.to_owned())
                    .unwrap_or_else(|| xid.ur_string())
            } else {
                registry
                    .participant(xid)
                    .and_then(|r| r.pet_name().map(|n| n.to_owned()))
                    .unwrap_or_else(|| xid.ur_string())
            };
            format_name_with_owner_marker(name, is_owner)
        })
        .collect()
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
