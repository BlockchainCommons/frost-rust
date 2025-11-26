use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, XID, XIDProvider};
use bc_envelope::prelude::*;
use bc_ur::prelude::UR;
use bc_xid::{XIDDocument, XIDVerifySignature};
use clap::{Args, Parser, Subcommand};
use frost_ed25519::{self as frost, Identifier};
use gstp::{
    SealedRequest, SealedRequestBehavior, SealedResponse,
    SealedResponseBehavior,
};
use rand_core::OsRng;
use tokio::runtime::Runtime;

use crate::{
    DkgGroupInvite, DkgInvitation,
    cmd::{
        registry::participants_file_path,
        storage::{
            StorageBackend, StorageClient, StorageSelection, StorageSelector,
        },
    },
    registry::{
        ContributionPaths, GroupParticipant, GroupRecord, GroupStatus,
        OwnerRecord, ParticipantRecord, Registry,
    },
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

#[derive(Debug, Clone, Args)]
#[doc(hidden)]
pub struct OptionalStorageSelector {
    /// Storage backend to use
    #[arg(long, short, value_enum)]
    storage: Option<StorageBackend>,

    /// Server/IPFS host (for --storage server)
    #[arg(long)]
    host: Option<String>,

    /// Port (for --storage server, --storage ipfs, or --storage hybrid)
    #[arg(long)]
    port: Option<u16>,
}

impl OptionalStorageSelector {
    pub fn resolve(&self) -> Result<Option<StorageSelection>> {
        if let Some(storage) = self.storage {
            let selector = StorageSelector {
                storage,
                host: self.host.clone(),
                port: self.port,
            };
            return Ok(Some(selector.resolve()?));
        }

        if self.host.is_some() || self.port.is_some() {
            bail!("--host/--port require --storage to select a Hubert backend");
        }

        Ok(None)
    }
}

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct InviteRespondArgs {
    #[command(flatten)]
    storage: OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Wait up to this many seconds for the invite to appear (when fetching
    /// from Hubert)
    #[arg(long = "timeout", value_name = "SECONDS")]
    timeout: Option<u64>,

    /// Optional ARID to use for the next response in the exchange; defaults to
    /// a new random ARID
    #[arg(long = "response-arid", value_name = "UR:ARID")]
    response_arid: Option<String>,

    /// Print the unsealed response envelope UR instead of the sealed envelope
    /// (local-only)
    #[arg(long = "unsealed")]
    unsealed: bool,

    /// Reject the invite with the provided reason (accepts by default)
    #[arg(long = "reject", value_name = "REASON")]
    reject_reason: Option<String>,

    /// Optionally require the invite to come from this sender (ur:xid or pet
    /// name in registry)
    #[arg(long = "sender", value_name = "SENDER")]
    sender: Option<String>,

    /// Invite ARID or envelope (ur:arid or ur:envelope)
    #[arg(value_name = "INVITE")]
    invite: String,
}

impl InviteRespondArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        if selection.is_none() && self.timeout.is_some() {
            bail!("--timeout requires Hubert storage parameters");
        }
        if selection.is_some() && self.unsealed {
            bail!("--unsealed cannot be used with Hubert storage options");
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
            .context("Registry owner with private keys is required")?
            .clone();
        let expected_sender = match &self.sender {
            Some(raw) => Some(resolve_sender(&registry, raw)?),
            None => None,
        };
        let next_response_arid = match &self.response_arid {
            Some(raw) => parse_arid_ur(raw)?,
            None => ARID::new(),
        };

        let invite_envelope = resolve_invite_envelope(
            selection.clone(),
            &self.invite,
            self.timeout,
        )?;

        let now = Date::now();
        let details = decode_invite_details(
            invite_envelope,
            now,
            &registry,
            owner.xid_document(),
            expected_sender,
        )?;

        let mut sorted_participants = details.participants.clone();
        sorted_participants.sort_by_key(|doc| doc.xid());
        let owner_index = sorted_participants
            .iter()
            .position(|doc| doc.xid() == owner.xid())
            .context("Invite does not include the registry owner")?;
        let identifier_index = u16::try_from(owner_index + 1)
            .context("Too many participants for identifiers")?;
        let identifier = Identifier::try_from(identifier_index)?;
        let total = u16::try_from(sorted_participants.len())
            .context("Too many participants for FROST identifiers")?;
        let min_signers = u16::try_from(details.invitation.min_signers())
            .context("min_signers does not fit into identifier space")?;

        let group_participants =
            build_group_participants(&registry, &owner, &sorted_participants)?;
        let coordinator = group_participant_from_registry(
            &registry,
            &owner,
            &details.invitation.sender(),
        )?;

        let mut contributions = ContributionPaths::default();
        let mut response_body = build_response_body(
            details.invitation.group_id(),
            owner.xid(),
            next_response_arid,
            None,
        )?;

        if self.reject_reason.is_none() {
            let (round1_secret, round1_package) =
                frost::keys::dkg::part1(identifier, total, min_signers, OsRng)?;
            contributions = persist_round1_state(
                &registry_path,
                &details.invitation.group_id(),
                &round1_secret,
                &round1_package,
            )?;
            response_body = build_response_body(
                details.invitation.group_id(),
                owner.xid(),
                next_response_arid,
                Some(&round1_package),
            )?;
        }

        let status = match &self.reject_reason {
            Some(reason) => {
                GroupStatus::Rejected { reason: Some(reason.clone()) }
            }
            None => GroupStatus::Accepted,
        };
        let mut group_record = GroupRecord::new(
            details.invitation.charter().to_owned(),
            details.invitation.min_signers(),
            coordinator,
            group_participants,
            details.invitation.request_id(),
            details.invitation.response_arid(),
            status.clone(),
        );
        group_record.set_contributions(contributions);
        group_record.set_next_response_arid(next_response_arid);
        registry.record_group(details.invitation.group_id(), group_record)?;
        registry.save(&registry_path)?;

        let signer_private_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Owner XID document has no signing keys")?;
        let mut sealed = if let Some(ref reason) = self.reject_reason {
            let error_body = Envelope::new("dkgInviteReject")
                .add_assertion("group", details.invitation.group_id())
                .add_assertion("response_arid", next_response_arid)
                .add_assertion("reason", reason.clone());
            SealedResponse::new_failure(
                details.invitation.request_id(),
                owner.xid_document().clone(),
            )
            .with_error(error_body.clone())
            .with_state(next_response_arid)
        } else {
            SealedResponse::new_success(
                details.invitation.request_id(),
                owner.xid_document().clone(),
            )
            .with_result(response_body.clone())
            .with_state(next_response_arid)
        };
        sealed = sealed
            .with_peer_continuation(details.invitation.peer_continuation());

        if let Some(selection) = selection {
            let response_envelope = sealed.to_envelope(
                Some(details.invitation.valid_until()),
                Some(signer_private_keys),
                Some(&details.invitation.sender()),
            )?;
            let response_target = details.invitation.response_arid();
            let envelope_to_send = response_envelope.clone();
            let runtime = Runtime::new()?;
            runtime.block_on(async move {
                let client = StorageClient::from_selection(selection).await?;
                client.put(&response_target, &envelope_to_send).await?;
                Ok::<(), anyhow::Error>(())
            })?;
        } else if self.unsealed {
            // Show the GSTP response structure without encryption
            let unsealed_envelope =
                sealed.to_envelope(None, Some(signer_private_keys), None)?;
            println!("{}", unsealed_envelope.ur_string());
        } else {
            let response_envelope = sealed.to_envelope(
                Some(details.invitation.valid_until()),
                Some(signer_private_keys),
                Some(&details.invitation.sender()),
            )?;
            println!("{}", response_envelope.ur_string());
        }

        Ok(())
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
    /// Compose or send a DKG invite
    Send(InviteSendArgs),
    /// Retrieve or inspect a DKG invite
    Receive(InviteReceiveArgs),
    /// Respond to a DKG invite
    Respond(InviteRespondArgs),
}

impl InviteArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            InviteCommands::Send(args) => args.exec(),
            InviteCommands::Receive(args) => args.exec(),
            InviteCommands::Respond(args) => args.exec(),
        }
    }
}

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct InviteSendArgs {
    #[command(flatten)]
    storage: OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Minimum signers required; defaults to participant count
    #[arg(long = "min-signers", value_name = "N")]
    min_signers: Option<usize>,

    /// Charter statement for the DKG group
    #[arg(long = "charter", value_name = "STRING", default_value = "")]
    charter: String,

    /// Print the unsealed invite envelope UR instead of the sealed envelope
    #[arg(long = "unsealed")]
    unsealed: bool,

    /// Participants to include, by pet name or ur:xid identifier
    #[arg(required = true, value_name = "PARTICIPANT")]
    participants: Vec<String>,
}

impl InviteSendArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        if selection.is_some() && self.unsealed {
            bail!("--unsealed cannot be used with Hubert storage options");
        }

        let invite = build_invite(
            self.registry,
            self.min_signers,
            self.charter,
            self.participants,
        )?;

        if let Some(selection) = selection {
            let envelope = invite.to_envelope()?;
            let arid = ARID::new();

            let runtime = Runtime::new()?;
            runtime.block_on(async move {
                let client = StorageClient::from_selection(selection).await?;
                client.put(&arid, &envelope).await?;
                Ok::<(), anyhow::Error>(())
            })?;

            println!("{}", arid.ur_string());
        } else if self.unsealed {
            let envelope = invite.to_unsealed_envelope()?;
            println!("{}", envelope.ur_string());
        } else {
            let envelope = invite.to_envelope()?;
            println!("{}", envelope.ur_string());
        }

        Ok(())
    }
}

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct InviteReceiveArgs {
    #[command(flatten)]
    storage: OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Wait up to this many seconds for the invite to appear
    #[arg(long = "timeout", value_name = "SECONDS")]
    timeout: Option<u64>,

    /// Suppress printing the invite envelope UR
    #[arg(long)]
    no_envelope: bool,

    /// Show invite details (charter, min signers, coordinator, participants)
    #[arg(long)]
    info: bool,

    /// Optionally require the invite to come from this sender (ur:xid or pet
    /// name in registry)
    #[arg(long = "sender", value_name = "SENDER")]
    sender: Option<String>,

    /// Invite ARID or envelope (ur:arid or ur:envelope)
    #[arg(value_name = "INVITE")]
    invite: String,
}

impl InviteReceiveArgs {
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

        let invite_envelope = resolve_invite_envelope(
            selection.clone(),
            &self.invite,
            self.timeout,
        )?;

        let now = Date::now();
        let details = decode_invite_details(
            invite_envelope.clone(),
            now,
            &registry,
            owner.xid_document(),
            expected_sender,
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
            println!("{}", invite_envelope.ur_string());
        }
        if self.info {
            println!("Charter: {}", details.invitation.charter());
            println!("Min signers: {}", details.invitation.min_signers());
            if let Some(name) = coordinator_name {
                println!("Coordinator: {}", name);
            }
            println!("Participants: {}", participant_names.join(", "));
        }

        Ok(())
    }
}

fn resolve_invite_envelope(
    selection: Option<StorageSelection>,
    invite: &str,
    timeout: Option<u64>,
) -> Result<Envelope> {
    if let Some(selection) = selection {
        if let Ok(arid) = parse_arid_ur(invite) {
            let runtime = Runtime::new()?;
            return runtime.block_on(async move {
                let client = StorageClient::from_selection(selection).await?;
                client
                    .get(&arid, timeout)
                    .await?
                    .context("Invite not found in Hubert storage")
            });
        }
        if timeout.is_some() {
            bail!(
                "--timeout is only valid when retrieving invites from Hubert"
            );
        }
        return parse_envelope_ur(invite);
    }

    if parse_arid_ur(invite).is_ok() {
        bail!(
            "Hubert storage parameters are required to retrieve invites by ARID"
        );
    }
    parse_envelope_ur(invite)
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
    participants: Vec<XIDDocument>,
}

fn decode_invite_details(
    invite: Envelope,
    now: Date,
    registry: &Registry,
    recipient: &XIDDocument,
    expected_sender: Option<XIDDocument>,
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

    let sender_document = sealed_request.sender().clone();
    if let Some(expected) = expected_sender.as_ref() {
        if sender_document.xid() != expected.xid() {
            bail!("Invite sender does not match expected sender");
        }
    } else {
        let sender_xid = sender_document.xid();
        let known_owner = registry
            .owner()
            .map(|owner| owner.xid() == sender_xid)
            .unwrap_or(false);
        let known_participant = registry.participant(&sender_xid).is_some();
        if !known_owner && !known_participant {
            bail!(
                "Invite sender not found in registry: {}",
                sender_xid.ur_string()
            );
        }
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
        .extract_object_for_parameter::<ARID>("group")?;
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
        expected_sender.as_ref(),
        recipient,
    )?;

    if response_arid.is_none() {
        bail!("Invite does not include a response ARID for this recipient");
    }

    Ok(InviteDetails { invitation, participants: participant_docs })
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

fn build_group_participants(
    registry: &Registry,
    owner: &OwnerRecord,
    participants: &[XIDDocument],
) -> Result<Vec<GroupParticipant>> {
    participants
        .iter()
        .map(|doc| group_participant_from_registry(registry, owner, doc))
        .collect()
}

fn group_participant_from_registry(
    registry: &Registry,
    owner: &OwnerRecord,
    document: &XIDDocument,
) -> Result<GroupParticipant> {
    let xid = document.xid();
    if xid == owner.xid() {
        return Ok(GroupParticipant::new(
            xid,
            owner.pet_name().map(|s| s.to_owned()),
        ));
    }
    let record = registry.participant(&xid).ok_or_else(|| {
        anyhow::anyhow!(
            "Invite participant not found in registry: {}",
            xid.ur_string()
        )
    })?;
    Ok(GroupParticipant::new(
        xid,
        record.pet_name().map(|s| s.to_owned()),
    ))
}

fn build_response_body(
    group_id: ARID,
    participant: XID,
    response_arid: ARID,
    round1_package: Option<&frost::keys::dkg::round1::Package>,
) -> Result<Envelope> {
    let mut envelope = Envelope::new("dkgInviteResponse")
        .add_assertion("group", group_id)
        .add_assertion("participant", participant)
        .add_assertion("response_arid", response_arid);
    if let Some(package) = round1_package {
        let encoded = serde_json::to_vec(package)?;
        let bstr = CBOR::to_byte_string(encoded.as_slice());
        envelope = envelope.add_assertion("round1_package", bstr);
    }
    Ok(envelope)
}

fn persist_round1_state(
    registry_path: &Path,
    group_id: &ARID,
    round1_secret: &frost::keys::dkg::round1::SecretPackage,
    round1_package: &frost::keys::dkg::round1::Package,
) -> Result<ContributionPaths> {
    let dir = group_state_dir(registry_path, group_id);
    fs::create_dir_all(&dir).with_context(|| {
        format!("Failed to create group state directory {}", dir.display())
    })?;
    let secret_path = dir.join("round1_secret.json");
    let package_path = dir.join("round1_package.json");
    fs::write(&secret_path, serde_json::to_vec_pretty(round1_secret)?)
        .with_context(|| {
            format!("Failed to write {}", secret_path.display())
        })?;
    fs::write(&package_path, serde_json::to_vec_pretty(round1_package)?)
        .with_context(|| {
            format!("Failed to write {}", package_path.display())
        })?;

    Ok(ContributionPaths {
        round1_secret: Some(secret_path.to_string_lossy().into_owned()),
        round1_package: Some(package_path.to_string_lossy().into_owned()),
        round2_secret: None,
        key_package: None,
    })
}

fn group_state_dir(registry_path: &Path, group_id: &ARID) -> PathBuf {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("group-state").join(group_id.hex())
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
    if is_owner { format!("* {name}") } else { name }
}
