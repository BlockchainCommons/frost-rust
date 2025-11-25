use std::{collections::HashSet, time::Duration};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, XID, XIDProvider};
use bc_envelope::prelude::*;
use bc_ur::prelude::UR;
use bc_xid::{XIDDocument, XIDVerifySignature};
use clap::{Parser, Subcommand};
use gstp::{SealedRequest, SealedRequestBehavior};
use tokio::runtime::Runtime;

use crate::{
    DkgGroupInvite, DkgInvitation,
    cmd::{
        registry::participants_file_path,
        storage::{StorageClient, StorageSelector},
    },
    registry::{ParticipantRecord, Registry},
};

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
#[doc(hidden)]
enum Commands {
    /// Create and display a DKG invite for selected participants
    Invite(InviteArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Invite(args) => args.exec(),
        }
    }
}

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct InviteArgs {
    #[command(subcommand)]
    command: InviteCommands,
}

#[derive(Debug, Subcommand)]
#[doc(hidden)]
enum InviteCommands {
    /// Compose a DKG invite for the given participants
    Compose(InviteShowArgs),
    /// Create a sealed DKG invite and store it in Hubert
    Send(InvitePutArgs),
    /// Retrieve and inspect a sealed DKG invite from Hubert
    View(InviteViewArgs),
}

impl InviteArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            InviteCommands::Compose(args) => args.exec(),
            InviteCommands::Send(args) => args.exec(),
            InviteCommands::View(args) => args.exec(),
        }
    }
}

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct InviteShowArgs {
    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Return a sealed invite envelope instead of the request envelope
    #[arg(long)]
    sealed: bool,

    /// Minimum signers required; defaults to participant count
    #[arg(long = "min-signers", value_name = "N")]
    min_signers: Option<usize>,

    /// Charter statement for the DKG session
    #[arg(long = "charter", value_name = "STRING", default_value = "")]
    charter: String,

    /// Participants to include, by pet name or ur:xid identifier
    #[arg(required = true, value_name = "PARTICIPANT")]
    participants: Vec<String>,
}

impl InviteShowArgs {
    pub fn exec(self) -> Result<()> {
        let invite = build_invite(
            self.registry,
            self.min_signers,
            self.charter,
            self.participants,
        )?;

        if self.sealed {
            let envelope = invite.to_envelope()?;
            println!("{}", envelope.ur_string());
        } else {
            let envelope = invite.to_request()?.request().to_envelope();
            println!("{}", envelope.ur_string());
        }

        Ok(())
    }
}

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct InvitePutArgs {
    #[command(flatten)]
    storage: StorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Minimum signers required; defaults to participant count
    #[arg(long = "min-signers", value_name = "N")]
    min_signers: Option<usize>,

    /// Charter statement for the DKG session
    #[arg(long = "charter", value_name = "STRING", default_value = "")]
    charter: String,

    /// Participants to include, by pet name or ur:xid identifier
    #[arg(required = true, value_name = "PARTICIPANT")]
    participants: Vec<String>,
}

impl InvitePutArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let invite = build_invite(
            self.registry,
            self.min_signers,
            self.charter,
            self.participants,
        )?;
        let envelope = invite.to_envelope()?;
        let arid = ARID::new();

        let runtime = Runtime::new()?;
        runtime.block_on(async move {
            let client = StorageClient::from_selection(selection).await?;
            client.put(&arid, &envelope).await?;
            println!("{}", arid.ur_string());
            Ok(())
        })
    }
}

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct InviteViewArgs {
    #[command(flatten)]
    storage: StorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Wait up to this many seconds for the invite to appear
    #[arg(long = "timeout", value_name = "SECONDS")]
    timeout: Option<u64>,

    /// ARID for the sealed invite (ur:arid)
    #[arg(value_name = "UR:ARID")]
    arid: String,

    /// Optional pre-fetched invite envelope (ur:envelope); skips Hubert retrieval when present
    #[arg(long = "envelope", value_name = "UR:ENVELOPE")]
    envelope: Option<String>,

    /// Show invite details (charter, min signers, coordinator, participants, reply ARID)
    #[arg(long)]
    info: bool,

    /// Suppress printing the invite envelope UR
    #[arg(long)]
    no_envelope: bool,

    /// Expected sender of the invite (ur:xid or pet name in registry)
    #[arg(value_name = "SENDER")]
    sender: String,
}

impl InviteViewArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let registry_path = participants_file_path(self.registry.clone())?;
        let registry = Registry::load(&registry_path).with_context(|| {
            format!("Failed to load registry at {}", registry_path.display())
        })?;
        let owner = registry
            .owner()
            .context("Registry owner with private keys is required")?
            .clone();
        let expected_sender = resolve_sender(&registry, self.sender.as_str())?;
        let arid = parse_arid_ur(&self.arid)?;

        let registry = registry;
        let runtime = Runtime::new()?;
        runtime.block_on(async move {
            let client = StorageClient::from_selection(selection).await?;
            let envelope = client
                .get(&arid, self.timeout)
                .await?
                .context("Invite not found in Hubert storage")?;

            let now = Date::now();
            let details = decode_invite_details(
                envelope,
                now,
                expected_sender,
                owner.xid_document(),
            )?;

            let participant_names = participant_names_from_registry(
                &registry,
                &details.participants,
                &owner.xid(),
                owner.pet_name(),
            )?;
            let coordinator_name =
                resolve_sender_name(&registry, &details.invitation.sender());

            if !self.no_envelope {
                println!("{}", details.invitation_envelope.ur_string());
            }
            if self.info {
                println!("Charter: {}", details.invitation.charter());
                println!("Min signers: {}", details.invitation.min_signers());
                if let Some(name) = coordinator_name {
                    println!("Coordinator: {}", name);
                }
                println!("Participants: {}", participant_names.join(", "));
                println!(
                    "Reply ARID: {}",
                    details.invitation.response_arid().ur_string()
                );
            }

            Ok(())
        })
    }
}

fn resolve_participants(
    registry: &Registry,
    inputs: &[String],
) -> Result<Vec<(XID, ParticipantRecord)>> {
    let mut seen_args = HashSet::new();
    let mut seen_xids = HashSet::new();
    let mut resolved = Vec::new();

    for raw in inputs {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            bail!("Participant identifier cannot be empty");
        }
        if !seen_args.insert(trimmed.to_owned()) {
            bail!("Duplicate participant argument: {trimmed}");
        }

        let (xid, record) = if let Ok(xid) = XID::from_ur_string(trimmed) {
            let record = registry.participant(&xid).with_context(|| {
                format!(
                    "Participant with XID {} not found in registry",
                    xid.ur_string()
                )
            })?;
            (xid, record.clone())
        } else {
            let (xid, record) = registry
                .participant_by_pet_name(trimmed)
                .with_context(|| {
                    format!("Participant with pet name '{trimmed}' not found")
                })?;
            (xid.to_owned(), record.clone())
        };

        if !seen_xids.insert(xid) {
            bail!(
                "Duplicate participant specified; multiple inputs resolve to {}",
                xid.ur_string()
            );
        }

        resolved.push((xid, record));
    }

    Ok(resolved)
}

fn build_invite(
    registry_arg: Option<String>,
    min_signers_arg: Option<usize>,
    charter: String,
    participants: Vec<String>,
) -> Result<DkgGroupInvite> {
    let registry_path = participants_file_path(registry_arg.clone())?;
    let registry = Registry::load(&registry_path).with_context(|| {
        format!("Failed to load registry at {}", registry_path.display())
    })?;

    let resolved = resolve_participants(&registry, &participants)?;
    let participant_docs: Vec<String> = resolved
        .iter()
        .map(|(_, record)| record.xid_document_ur().to_owned())
        .collect();
    let response_arids: Vec<ARID> =
        (0..participant_docs.len()).map(|_| ARID::new()).collect();

    let participant_count = participant_docs.len();
    if participant_count < 2 {
        bail!("At least two participants are required for a DKG invite");
    }
    let min_signers = min_signers_arg.unwrap_or(participant_count);
    if min_signers < 2 {
        bail!("--min-signers must be at least 2");
    }
    if min_signers > participant_count {
        bail!("--min-signers cannot exceed participant count");
    }

    DkgGroupInvite::new(
        ARID::new(),
        registry
            .owner()
            .context("Registry owner is required to issue invites")?
            .xid_document()
            .clone(),
        ARID::new(),
        Date::now(),
        Date::with_duration_from_now(Duration::from_secs(60 * 60)),
        min_signers,
        charter,
        participant_docs,
        response_arids,
    )
}

struct InviteDetails {
    invitation: DkgInvitation,
    invitation_envelope: Envelope,
    participants: Vec<XIDDocument>,
}

fn decode_invite_details(
    invite: Envelope,
    now: Date,
    expected_sender: XIDDocument,
    recipient: &XIDDocument,
) -> Result<InviteDetails> {
    let recipient_private_keys =
        recipient.inception_private_keys().ok_or_else(|| {
            anyhow::anyhow!(
                "Recipient XID document has no inception private keys"
            )
        })?;

    let sealed_request = SealedRequest::try_from_envelope(
        &invite,
        None,
        Some(now),
        recipient_private_keys,
    )?;

    if sealed_request.sender().xid() != expected_sender.xid() {
        bail!("Invite sender does not match expected sender");
    }
    if sealed_request.request().function() != &Function::from("dkgGroupInvite")
    {
        bail!("Unexpected invite function");
    }

    let valid_until: Date = sealed_request
        .request()
        .extract_object_for_parameter("validUntil")?;
    if valid_until <= now {
        bail!("Invitation expired");
    }

    let min_signers: usize = sealed_request
        .request()
        .extract_object_for_parameter("minSigners")?;
    sealed_request
        .request()
        .extract_object_for_parameter::<String>("charter")?;
    sealed_request
        .request()
        .extract_object_for_parameter::<ARID>("session")?;
    let participant_objects = sealed_request
        .request()
        .objects_for_parameter("participant");
    if min_signers < 2 {
        bail!("min_signers must be at least 2");
    }
    if min_signers > participant_objects.len() {
        bail!("min_signers exceeds participant count");
    }

    let mut participant_docs = Vec::new();
    let mut response_arid: Option<ARID> = None;
    for participant in participant_objects {
        let xid_doc_envelope = participant.try_unwrap()?;
        let xid_document = XIDDocument::from_envelope(
            &xid_doc_envelope,
            None,
            XIDVerifySignature::Inception,
        )?;
        if xid_document.xid() == recipient.xid() {
            let encrypted_response_arid =
                participant.object_for_predicate("response_arid")?;
            let response_arid_envelope = encrypted_response_arid
                .decrypt_to_recipient(recipient_private_keys)?;
            response_arid =
                Some(response_arid_envelope.extract_subject::<ARID>()?);
        }
        participant_docs.push(xid_document);
    }

    let invitation = DkgInvitation::from_invite(
        invite.clone(),
        now,
        &expected_sender,
        recipient,
    )?;

    if response_arid.is_none() {
        bail!("Invite does not include a response ARID for this recipient");
    }

    Ok(InviteDetails {
        invitation,
        invitation_envelope: invite,
        participants: participant_docs,
    })
}

fn participant_names_from_registry(
    registry: &Registry,
    participants: &[XIDDocument],
    owner_xid: &XID,
    owner_pet_name: Option<&str>,
) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for document in participants {
        let xid = document.xid();
        let is_owner = xid == *owner_xid;
        let name = if is_owner {
            owner_pet_name
                .map(|n| n.to_owned())
                .unwrap_or_else(|| xid.ur_string())
        } else {
            let record = registry.participant(&xid).ok_or_else(|| {
                anyhow::anyhow!(
                    "Invite participant not found in registry: {}",
                    xid.ur_string()
                )
            })?;
            record
                .pet_name()
                .map(|n| n.to_owned())
                .unwrap_or_else(|| xid.ur_string())
        };
        names.push(format_name_with_owner_marker(name, is_owner));
    }
    Ok(names)
}

fn resolve_sender(registry: &Registry, input: &str) -> Result<XIDDocument> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("Sender is required");
    }

    if let Ok(xid) = XID::from_ur_string(trimmed) {
        let record = registry.participant(&xid).with_context(|| {
            format!("Sender with XID {} not found", xid.ur_string())
        })?;
        Ok(record.xid_document().clone())
    } else {
        let (_, record) =
            registry.participant_by_pet_name(trimmed).with_context(|| {
                format!("Sender with pet name '{trimmed}' not found")
            })?;
        Ok(record.xid_document().clone())
    }
}

fn resolve_sender_name(
    registry: &Registry,
    sender: &XIDDocument,
) -> Option<String> {
    if let Some(owner) = registry.owner()
        && owner.xid_document().xid() == sender.xid()
    {
        let name = owner
            .pet_name()
            .map(|s| s.to_owned())
            .unwrap_or_else(|| sender.xid().ur_string());
        return Some(format_name_with_owner_marker(name, true));
    }
    registry.participant(&sender.xid()).map(|record| {
        let name = record
            .pet_name()
            .map(|n| n.to_owned())
            .unwrap_or_else(|| record.xid().ur_string());
        format_name_with_owner_marker(name, false)
    })
}

#[allow(dead_code)]
fn parse_envelope_ur(input: &str) -> Result<Envelope> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("Invite envelope is required");
    }
    let ur = UR::from_ur_string(trimmed)
        .with_context(|| format!("Failed to parse envelope UR: {trimmed}"))?;
    if ur.ur_type_str() != "envelope" {
        bail!("Expected a ur:envelope, found ur:{}", ur.ur_type_str());
    }
    Envelope::from_tagged_cbor(ur.cbor())
        .or_else(|_| Envelope::from_untagged_cbor(ur.cbor()))
        .context("Invalid envelope payload")
}

fn parse_arid_ur(input: &str) -> Result<ARID> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("Invite ARID is required");
    }
    let ur = UR::from_ur_string(trimmed)
        .with_context(|| format!("Failed to parse ARID UR: {trimmed}"))?;
    if ur.ur_type_str() != "arid" {
        bail!("Expected a ur:arid, found ur:{}", ur.ur_type_str());
    }
    let cbor = ur.cbor();
    ARID::try_from(cbor.clone()).or_else(|_| {
        let bytes =
            CBOR::try_into_byte_string(cbor).context("Invalid ARID payload")?;
        ARID::from_data_ref(bytes).context("Invalid ARID payload")
    })
}

fn format_name_with_owner_marker(name: String, is_owner: bool) -> String {
    if is_owner {
        format!("* {name}")
    } else {
        name
    }
}
