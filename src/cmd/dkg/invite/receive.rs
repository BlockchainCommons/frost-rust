use anyhow::{Context, Result, bail};
use bc_components::{ARID, XIDProvider};
use bc_envelope::prelude::*;
use bc_xid::{XIDDocument, XIDVerifySignature};
use clap::Parser;
use gstp::{SealedRequest, SealedRequestBehavior};
use tokio::runtime::Runtime;

use crate::cmd::dkg::common::{
    OptionalStorageSelector, parse_arid_ur, parse_envelope_ur,
    participant_names_from_registry, resolve_sender, resolve_sender_name,
};
use crate::{
    DkgInvitation,
    cmd::{
        registry::participants_file_path,
        storage::{StorageClient, StorageSelection},
    },
    registry::Registry,
};

/// Retrieve or inspect a DKG invite.
#[derive(Debug, Parser)]
#[group(skip)]
pub struct CommandArgs {
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

pub struct InviteDetails {
    pub invitation: DkgInvitation,
    pub participants: Vec<XIDDocument>,
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

pub fn decode_invite_details(
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
