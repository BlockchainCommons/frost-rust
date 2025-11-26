use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, XID, XIDProvider};
use bc_envelope::prelude::*;
use clap::Parser;
use frost_ed25519::{self as frost, Identifier};
use gstp::{SealedResponse, SealedResponseBehavior};
use rand_core::OsRng;
use tokio::runtime::Runtime;

use super::{
    common::{
        OptionalStorageSelector, build_group_participants, parse_arid_ur,
        resolve_sender,
    },
    receive::decode_invite_details,
};
use crate::{
    cmd::{
        registry::participants_file_path,
        storage::{StorageClient, StorageSelection},
    },
    registry::{
        ContributionPaths, GroupParticipant, GroupRecord, OwnerRecord, Registry,
    },
};

/// Respond to a DKG invite.
#[derive(Debug, Parser)]
#[group(skip)]
pub struct CommandArgs {
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

    /// Print the preview response envelope UR instead of the sealed envelope
    /// (local-only)
    #[arg(long = "preview")]
    preview: bool,

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

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        if selection.is_none() && self.timeout.is_some() {
            bail!("--timeout requires Hubert storage parameters");
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

        // Build the response body
        // Only generate actual round1 state if we're going to post to storage
        let is_posting = selection.is_some();

        #[allow(unused_variables)]
        let (response_body, round1_package_opt) = if self
            .reject_reason
            .is_none()
            && is_posting
        {
            // Actually posting - generate and persist round1 state
            let (round1_secret, round1_package) =
                frost::keys::dkg::part1(identifier, total, min_signers, OsRng)?;
            let contributions = persist_round1_state(
                &registry_path,
                &details.invitation.group_id(),
                &round1_secret,
                &round1_package,
            )?;
            let body = build_response_body(
                details.invitation.group_id(),
                owner.xid(),
                next_response_arid,
                Some(&round1_package),
            )?;

            let mut group_record = GroupRecord::new(
                details.invitation.charter().to_owned(),
                details.invitation.min_signers(),
                coordinator.clone(),
                group_participants.clone(),
            );
            group_record.set_contributions(contributions);
            // Set the ARID where we're listening for the Round 2 request
            group_record.set_listening_at_arid(next_response_arid);
            registry
                .record_group(details.invitation.group_id(), group_record)?;
            registry.save(&registry_path)?;

            (body, Some(round1_package))
        } else if self.reject_reason.is_none() {
            // Preview mode - generate dummy round1 for envelope structure only
            let (_, round1_package) =
                frost::keys::dkg::part1(identifier, total, min_signers, OsRng)?;
            let body = build_response_body(
                details.invitation.group_id(),
                owner.xid(),
                next_response_arid,
                Some(&round1_package),
            )?;
            (body, None)
        } else {
            // Rejecting - no round1 needed
            let body = build_response_body(
                details.invitation.group_id(),
                owner.xid(),
                next_response_arid,
                None,
            )?;
            (body, None)
        };

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
        } else if self.preview {
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

fn resolve_invite_envelope(
    selection: Option<StorageSelection>,
    invite: &str,
    timeout: Option<u64>,
) -> Result<Envelope> {
    use super::common::parse_envelope_ur;

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

fn group_participant_from_registry(
    registry: &Registry,
    owner: &OwnerRecord,
    document: &bc_xid::XIDDocument,
) -> Result<GroupParticipant> {
    let xid = document.xid();
    if xid == owner.xid() {
        return Ok(GroupParticipant::new(xid));
    }
    if registry.participant(&xid).is_none() {
        anyhow::bail!(
            "Invite participant not found in registry: {}",
            xid.ur_string()
        );
    }
    Ok(GroupParticipant::new(xid))
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
