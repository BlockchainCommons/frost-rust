use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, Digest, XID};
use bc_envelope::prelude::*;
use bc_xid::XIDDocument;
use clap::Parser;
use gstp::SealedRequest;
use tokio::runtime::Runtime;

use crate::{
    cmd::{
        is_verbose, registry::participants_file_path, storage::StorageClient,
    },
    registry::{GroupParticipant, Registry},
};

/// Start a threshold signing session (coordinator only).
#[derive(Debug, Parser)]
#[group(skip)]
pub struct CommandArgs {
    #[command(flatten)]
    storage: crate::cmd::dkg::OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Print the preview request envelope UR instead of sending
    #[arg(long = "preview")]
    preview: bool,

    /// Path to a file containing the target envelope UR (will be signed)
    #[arg(long = "target", value_name = "PATH")]
    target_envelope: String,

    /// Group ID to sign with
    #[arg(value_name = "GROUP_ID")]
    group_id: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        if selection.is_some() && self.preview {
            bail!("--preview cannot be used with Hubert storage options");
        }

        let registry_path = participants_file_path(self.registry.clone())?;
        let registry = Registry::load(&registry_path).with_context(|| {
            format!("Failed to load registry at {}", registry_path.display())
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
                "Only the coordinator can start signing. \
                 Coordinator: {}, Owner: {}",
                group_record.coordinator().xid().ur_string(),
                owner.xid().ur_string()
            );
        }

        // Load target envelope to sign
        let target_envelope = load_envelope_from_path(&self.target_envelope)
            .with_context(|| {
                format!(
                    "Failed to load target envelope from {}",
                    self.target_envelope
                )
            })?;
        let _target_digest: Digest = target_envelope.subject().digest();

        // Build participant set (signers): group participants only
        let participants: Vec<GroupParticipant> =
            group_record.participants().to_vec();

        // Gather XID documents for all participants
        let mut recipient_docs: Vec<XIDDocument> = Vec::new();
        for participant in &participants {
            let xid = participant.xid();
            if *xid == owner.xid() {
                recipient_docs.push(owner.xid_document().clone());
            } else {
                let doc = registry
                    .participant(xid)
                    .map(|r| r.xid_document().clone())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Participant {} not found in registry",
                            xid.ur_string()
                        )
                    })?;
                recipient_docs.push(doc);
            }
        }

        let signer_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Coordinator XID document has no signing keys")?;

        // ARIDs
        let session_id = ARID::new();
        let start_arid = ARID::new();

        // Per-participant ARIDs
        let mut commit_arids: HashMap<XID, ARID> = HashMap::new();
        let mut share_arids: HashMap<XID, ARID> = HashMap::new();
        for participant in &participants {
            commit_arids.insert(*participant.xid(), ARID::new());
            share_arids.insert(*participant.xid(), ARID::new());
        }

        // Build request
        let valid_until =
            Date::with_duration_from_now(Duration::from_secs(60 * 60));
        let mut request = SealedRequest::new(
            "signCommit",
            session_id,
            owner.xid_document().clone(),
        )
        .with_parameter("group", group_id)
        .with_parameter("session", session_id)
        .with_parameter("target", target_envelope.clone())
        .with_parameter("minSigners", group_record.min_signers() as u64)
        .with_date(Date::now())
        .with_parameter("validUntil", valid_until);

        for participant in &participants {
            let xid = participant.xid();
            let participant_doc = if *xid == owner.xid() {
                owner.xid_document().clone()
            } else {
                registry
                    .participant(xid)
                    .map(|r| r.xid_document().clone())
                    .context("Participant not found in registry")?
            };
            let encryption_key =
                participant_doc.encryption_key().ok_or_else(|| {
                    anyhow::anyhow!(
                        "Participant XID document has no encryption key"
                    )
                })?;
            let response_arid =
                commit_arids.get(xid).expect("commit ARID present");
            let encrypted_response_arid = response_arid
                .to_envelope()
                .encrypt_to_recipient(encryption_key);
            let participant_entry = Envelope::new(*xid)
                .add_assertion("response_arid", encrypted_response_arid);
            request = request.with_parameter("participant", participant_entry);
        }

        // Persist session state (regardless of posting or preview) to aid later
        // phases
        let signing_dir =
            signing_state_dir(&registry_path, &group_id, &session_id);
        let start_state_path = signing_dir.join("start.json");
        let mut participants_map = serde_json::Map::new();
        for participant in &participants {
            let xid = participant.xid();
            let mut entry = serde_json::Map::new();
            entry.insert(
                "commit_arid".to_string(),
                serde_json::Value::String(
                    commit_arids.get(xid).unwrap().ur_string(),
                ),
            );
            entry.insert(
                "share_arid".to_string(),
                serde_json::Value::String(
                    share_arids.get(xid).unwrap().ur_string(),
                ),
            );
            participants_map
                .insert(xid.ur_string(), serde_json::Value::Object(entry));
        }
        let mut root = serde_json::Map::new();
        root.insert(
            "session_id".to_string(),
            serde_json::Value::String(session_id.ur_string()),
        );
        root.insert(
            "start_arid".to_string(),
            serde_json::Value::String(start_arid.ur_string()),
        );
        root.insert(
            "group".to_string(),
            serde_json::Value::String(group_id.ur_string()),
        );
        root.insert(
            "min_signers".to_string(),
            serde_json::Value::Number(serde_json::Number::from(
                group_record.min_signers(),
            )),
        );
        root.insert(
            "participants".to_string(),
            serde_json::Value::Object(participants_map),
        );
        root.insert(
            "target".to_string(),
            serde_json::Value::String(target_envelope.ur_string()),
        );

        // Build envelope
        let recipient_refs: Vec<&XIDDocument> = recipient_docs.iter().collect();
        let sealed_envelope = request.to_envelope_for_recipients(
            Some(valid_until),
            Some(signer_keys),
            &recipient_refs,
        )?;

        if self.preview {
            let unsealed =
                request.to_envelope(None, Some(signer_keys), None)?;
            println!("{}", unsealed.ur_string());
            return Ok(());
        }

        // Persist session state (only on real send)
        fs::create_dir_all(&signing_dir)?;
        fs::write(&start_state_path, serde_json::to_vec_pretty(&root)?)?;

        let selection =
            selection.context("Hubert storage is required for sign start")?;
        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        if is_verbose() {
            eprintln!(
                "Posting signCommit request to {}",
                start_arid.ur_string()
            );
        }

        runtime.block_on(async {
            client.put(&start_arid, &sealed_envelope).await
        })?;

        println!("{}", start_arid.ur_string());

        Ok(())
    }
}

fn load_envelope_from_path(path: &str) -> Result<Envelope> {
    let data = fs::read_to_string(path).with_context(|| {
        format!("Failed to read target envelope from {path}")
    })?;
    let trimmed = data.trim();
    Envelope::from_ur_string(trimmed)
        .with_context(|| "Target envelope is not a valid UR".to_string())
}

fn parse_arid_ur(input: &str) -> Result<ARID> {
    use bc_ur::prelude::UR;

    let ur = UR::from_ur_string(input)
        .with_context(|| format!("Invalid UR string: {input}"))?;
    if ur.ur_type_str() != "arid" {
        bail!("Expected ur:arid, found ur:{}", ur.ur_type_str());
    }
    let cbor = ur.cbor();
    ARID::try_from(cbor.clone()).or_else(|_| {
        let bytes = CBOR::try_into_byte_string(cbor)
            .context("ARID is not a byte string")?;
        ARID::from_data_ref(bytes).context("Invalid ARID data")
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
