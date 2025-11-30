use std::{collections::HashMap, fs, time::Duration};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, Digest, XID};
use bc_envelope::prelude::*;
use bc_xid::XIDDocument;
use clap::Parser;
use gstp::SealedRequest;
use tokio::runtime::Runtime;

use crate::{
    cmd::{
        dkg::common::parse_arid_ur, is_verbose,
        registry::participants_file_path, sign::common::signing_state_dir,
        storage::StorageClient,
    },
    registry::{GroupParticipant, GroupRecord, OwnerRecord, Registry},
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

        validate_coordinator(&group_record, &owner)?;

        let target_envelope = load_envelope_from_path(&self.target_envelope)?;
        let _target_digest: Digest = target_envelope.subject().digest();

        let participants: Vec<GroupParticipant> =
            group_record.participants().to_vec();

        let recipient_docs =
            gather_recipient_documents(&participants, &owner, &registry)?;

        let signer_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Coordinator XID document has no signing keys")?;

        // Generate ARIDs for session
        let session_arids = SessionArids::new(&participants);

        // Build request
        let valid_until =
            Date::with_duration_from_now(Duration::from_secs(60 * 60));
        let ctx = SignInviteContext {
            arids: &session_arids,
            group_id: &group_id,
            target_envelope: &target_envelope,
            group_record: &group_record,
            owner: &owner,
            registry: &registry,
            participants: &participants,
            valid_until,
        };
        let request = build_sign_invite_request(&ctx)?;

        // Build state for persistence
        let state_json = build_session_state_json(
            &session_arids,
            &group_id,
            &group_record,
            &participants,
            &target_envelope,
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

        // Persist and send
        let signing_dir = signing_state_dir(
            &registry_path,
            &group_id,
            &session_arids.session_id,
        );
        persist_session_state(&signing_dir, &state_json)?;

        let selection =
            selection.context("Hubert storage is required for sign start")?;
        post_to_hubert(
            &selection,
            &session_arids.start_arid,
            &sealed_envelope,
        )?;

        println!("{}", session_arids.start_arid.ur_string());

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Session ARID management
// -----------------------------------------------------------------------------

struct SessionArids {
    session_id: ARID,
    start_arid: ARID,
    commit_arids: HashMap<XID, ARID>,
    share_arids: HashMap<XID, ARID>,
}

impl SessionArids {
    fn new(participants: &[GroupParticipant]) -> Self {
        let mut commit_arids = HashMap::new();
        let mut share_arids = HashMap::new();
        for participant in participants {
            commit_arids.insert(*participant.xid(), ARID::new());
            share_arids.insert(*participant.xid(), ARID::new());
        }
        Self {
            session_id: ARID::new(),
            start_arid: ARID::new(),
            commit_arids,
            share_arids,
        }
    }
}

// -----------------------------------------------------------------------------
// Validation
// -----------------------------------------------------------------------------

fn validate_coordinator(
    group_record: &GroupRecord,
    owner: &OwnerRecord,
) -> Result<()> {
    if group_record.coordinator().xid() != &owner.xid() {
        bail!(
            "Only the coordinator can start signing. \
             Coordinator: {}, Owner: {}",
            group_record.coordinator().xid().ur_string(),
            owner.xid().ur_string()
        );
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// Participant document gathering
// -----------------------------------------------------------------------------

fn gather_recipient_documents(
    participants: &[GroupParticipant],
    owner: &OwnerRecord,
    registry: &Registry,
) -> Result<Vec<XIDDocument>> {
    let mut recipient_docs = Vec::new();
    for participant in participants {
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
    Ok(recipient_docs)
}

// -----------------------------------------------------------------------------
// Request building
// -----------------------------------------------------------------------------

struct SignInviteContext<'a> {
    arids: &'a SessionArids,
    group_id: &'a ARID,
    target_envelope: &'a Envelope,
    group_record: &'a GroupRecord,
    owner: &'a OwnerRecord,
    registry: &'a Registry,
    participants: &'a [GroupParticipant],
    valid_until: Date,
}

fn build_sign_invite_request(
    ctx: &SignInviteContext<'_>,
) -> Result<SealedRequest> {
    let mut request = SealedRequest::new(
        "signInvite",
        ctx.arids.session_id,
        ctx.owner.xid_document().clone(),
    )
    .with_parameter("group", *ctx.group_id)
    .with_parameter("session", ctx.arids.session_id)
    .with_parameter("target", ctx.target_envelope.clone())
    .with_parameter("minSigners", ctx.group_record.min_signers() as u64)
    .with_date(Date::now())
    .with_parameter("validUntil", ctx.valid_until);

    for participant in ctx.participants {
        let xid = participant.xid();
        let participant_doc = if *xid == ctx.owner.xid() {
            ctx.owner.xid_document().clone()
        } else {
            ctx.registry
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
        let response_arid = ctx
            .arids
            .commit_arids
            .get(xid)
            .expect("commit ARID present");
        let encrypted_response_arid = response_arid
            .to_envelope()
            .encrypt_to_recipient(encryption_key);
        let participant_entry = Envelope::new(*xid)
            .add_assertion("response_arid", encrypted_response_arid);
        request = request.with_parameter("participant", participant_entry);
    }

    Ok(request)
}

// -----------------------------------------------------------------------------
// State persistence
// -----------------------------------------------------------------------------

fn build_session_state_json(
    arids: &SessionArids,
    group_id: &ARID,
    group_record: &GroupRecord,
    participants: &[GroupParticipant],
    target_envelope: &Envelope,
) -> serde_json::Map<String, serde_json::Value> {
    let mut participants_map = serde_json::Map::new();
    for participant in participants {
        let xid = participant.xid();
        let mut entry = serde_json::Map::new();
        entry.insert(
            "commit_arid".to_string(),
            serde_json::Value::String(
                arids.commit_arids.get(xid).unwrap().ur_string(),
            ),
        );
        entry.insert(
            "share_arid".to_string(),
            serde_json::Value::String(
                arids.share_arids.get(xid).unwrap().ur_string(),
            ),
        );
        participants_map
            .insert(xid.ur_string(), serde_json::Value::Object(entry));
    }

    let mut root = serde_json::Map::new();
    root.insert(
        "session_id".to_string(),
        serde_json::Value::String(arids.session_id.ur_string()),
    );
    root.insert(
        "start_arid".to_string(),
        serde_json::Value::String(arids.start_arid.ur_string()),
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

    root
}

fn persist_session_state(
    signing_dir: &std::path::Path,
    state_json: &serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    fs::create_dir_all(signing_dir)?;
    let start_state_path = signing_dir.join("start.json");
    fs::write(&start_state_path, serde_json::to_vec_pretty(state_json)?)?;
    Ok(())
}

// -----------------------------------------------------------------------------
// Hubert posting
// -----------------------------------------------------------------------------

fn post_to_hubert(
    selection: &crate::cmd::storage::StorageSelection,
    arid: &ARID,
    envelope: &Envelope,
) -> Result<()> {
    let runtime = Runtime::new()?;
    let client = runtime.block_on(async {
        StorageClient::from_selection(selection.clone()).await
    })?;

    if is_verbose() {
        eprintln!("Posting signInvite request to {}", arid.ur_string());
    }

    runtime.block_on(async { client.put(arid, envelope).await })?;

    Ok(())
}

// -----------------------------------------------------------------------------
// File loading
// -----------------------------------------------------------------------------

fn load_envelope_from_path(path: &str) -> Result<Envelope> {
    let data = fs::read_to_string(path).with_context(|| {
        format!("Failed to read target envelope from {path}")
    })?;
    let trimmed = data.trim();
    Envelope::from_ur_string(trimmed)
        .with_context(|| format!("Failed to load target envelope from {path}"))
}
