use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, XID};
use bc_envelope::prelude::*;
use bc_xid::XIDDocument;
use clap::Parser;
use frost_ed25519 as frost;
use gstp::SealedRequest;
use tokio::runtime::Runtime;

use super::super::common::OptionalStorageSelector;
use crate::{
    cmd::{
        is_verbose, registry::participants_file_path, storage::StorageClient,
    },
    registry::{PendingRequests, Registry},
};

/// Send finalize packages to all participants (coordinator only).
#[derive(Debug, Parser)]
#[group(skip)]
pub struct CommandArgs {
    #[command(flatten)]
    storage: OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Print the preview request envelope UR instead of sending
    #[arg(long = "preview")]
    preview: bool,

    /// Group ID to send finalize packages for
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

        let group_id = parse_arid_ur(&self.group_id)?;
        let group_record = registry
            .group(&group_id)
            .context("Group not found in registry")?
            .clone();

        // Verify coordinator
        if group_record.coordinator().xid() != &owner.xid() {
            bail!(
                "Only the coordinator can send finalize packages. \
                 Coordinator: {}, Owner: {}",
                group_record.coordinator().xid().ur_string(),
                owner.xid().ur_string()
            );
        }

        // Load collected round2 packages
        let state_dir = group_state_dir(&registry_path, &group_id);
        let collected_path = state_dir.join("collected_round2.json");
        if !collected_path.exists() {
            bail!(
                "Round 2 responses not found at {}. \
                 Run 'frost dkg round2 collect' first.",
                collected_path.display()
            );
        }

        let collected_json: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&fs::read(&collected_path).with_context(
                || format!("Failed to read {}", collected_path.display()),
            )?)
            .context("Failed to parse collected_round2.json")?;

        // Build map: sender_xid -> (response_arid, Vec<(recipient, package)>)
        let mut round2_packages: HashMap<
            XID,
            (ARID, Vec<(XID, frost::keys::dkg::round2::Package)>),
        > = HashMap::new();
        for (sender_str, value) in collected_json {
            let sender_xid = XID::from_ur_string(&sender_str)
                .context("Invalid sender XID in collected_round2.json")?;
            let obj = value
                .as_object()
                .context("Expected object for sender entry")?;
            let response_arid_raw = obj
                .get("response_arid")
                .and_then(|v| v.as_str())
                .context("Missing response_arid")?;
            let response_arid = parse_arid_ur(response_arid_raw)?;
            let packages_obj = obj
                .get("packages")
                .and_then(|v| v.as_object())
                .context("Missing packages object")?;
            let mut packages = Vec::new();
            for (recipient_str, pkg_value) in packages_obj {
                let recipient_xid = XID::from_ur_string(recipient_str)
                    .context(
                        "Invalid recipient XID in collected_round2.json",
                    )?;
                let pkg: frost::keys::dkg::round2::Package =
                    serde_json::from_value(pkg_value.clone())
                        .context("Failed to parse round2 package")?;
                packages.push((recipient_xid, pkg));
            }
            round2_packages.insert(sender_xid, (response_arid, packages));
        }

        // Pending requests: where to POST finalize messages
        let pending_requests = group_record.pending_requests();
        if pending_requests.is_empty() {
            bail!(
                "No pending requests for this group. \
                 Did you run 'frost dkg round2 collect'?"
            );
        }

        let coordinator_doc = owner.xid_document();
        let signer_keys = coordinator_doc
            .inception_private_keys()
            .context("Coordinator XID document has no signing keys")?;
        let valid_until =
            Date::with_duration_from_now(Duration::from_secs(60 * 60));

        // Build participant info: (xid, send_to_arid, collect_from_arid)
        let participant_info: Vec<(XID, ARID, ARID)> = pending_requests
            .iter_send()
            .map(|(xid, send_to_arid)| {
                let collect_from_arid = ARID::new(); // where they'll post finalize response
                Ok((*xid, *send_to_arid, collect_from_arid))
            })
            .collect::<Result<Vec<_>>>()?;

        // Build new pending_requests for finalize response collection
        let mut new_pending = PendingRequests::new();
        for (xid, _, collect_from_arid) in &participant_info {
            new_pending.add_collect_only(*xid, *collect_from_arid);
        }

        if let Some(selection) = selection {
            let runtime = Runtime::new()?;
            let client = runtime.block_on(async {
                StorageClient::from_selection(selection).await
            })?;

            if is_verbose() {
                eprintln!(
                    "Sending finalize packages to {} participants...",
                    participant_info.len()
                );
            }

            for (recipient_xid, send_to_arid, collect_from_arid) in
                &participant_info
            {
                let recipient_name = registry
                    .participant(recipient_xid)
                    .and_then(|r| r.pet_name().map(|s| s.to_owned()))
                    .unwrap_or_else(|| recipient_xid.ur_string());

                if is_verbose() {
                    eprint!("  {} ... ", recipient_name);
                }

                let packages_for_recipient = gather_packages_for_recipient(
                    recipient_xid,
                    &round2_packages,
                )?;

                let request = build_finalize_request_for_participant(
                    coordinator_doc,
                    &group_id,
                    *collect_from_arid,
                    &packages_for_recipient,
                )?;

                let sealed_envelope = request.to_envelope_for_recipients(
                    Some(valid_until),
                    Some(signer_keys),
                    &[recipient_doc(recipient_xid, &registry)?],
                )?;

                runtime.block_on(async {
                    client.put(send_to_arid, &sealed_envelope).await
                })?;

                if is_verbose() {
                    eprintln!("ok");
                }
            }

            // Update pending_requests for finalize collection
            let group_record = registry
                .group_mut(&group_id)
                .context("Group not found in registry")?;
            group_record.set_pending_requests(new_pending);
            registry.save(&registry_path)?;

            if is_verbose() {
                eprintln!();
                eprintln!("Sent {} finalize requests.", participant_info.len());
            }
        } else if self.preview {
            // Show a single preview request (signed, not encrypted)
            let (_, _, collect_from_arid) = &participant_info[0];
            let packages_for_recipient = gather_packages_for_recipient(
                &participant_info[0].0,
                &round2_packages,
            )?;
            let request = build_finalize_request_for_participant(
                coordinator_doc,
                &group_id,
                *collect_from_arid,
                &packages_for_recipient,
            )?;

            let unsealed_envelope = request.to_envelope(
                Some(valid_until),
                Some(signer_keys),
                None,
            )?;
            println!("{}", unsealed_envelope.ur_string());
        } else {
            // Sealed but not sent - show each participant's sealed envelope
            for (recipient_xid, _, collect_from_arid) in &participant_info {
                let recipient_name = registry
                    .participant(recipient_xid)
                    .and_then(|r| r.pet_name().map(|s| s.to_owned()))
                    .unwrap_or_else(|| recipient_xid.ur_string());

                let packages_for_recipient = gather_packages_for_recipient(
                    recipient_xid,
                    &round2_packages,
                )?;

                let request = build_finalize_request_for_participant(
                    coordinator_doc,
                    &group_id,
                    *collect_from_arid,
                    &packages_for_recipient,
                )?;

                let sealed_envelope = request.to_envelope_for_recipients(
                    Some(valid_until),
                    Some(signer_keys),
                    &[recipient_doc(recipient_xid, &registry)?],
                )?;

                eprintln!("# {}", recipient_name);
                println!("{}", sealed_envelope.ur_string());
            }
        }

        Ok(())
    }
}

fn recipient_doc<'a>(
    recipient: &XID,
    registry: &'a Registry,
) -> Result<&'a XIDDocument> {
    registry
        .participant(recipient)
        .map(|r| r.xid_document())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Participant {} not found in registry",
                recipient.ur_string()
            )
        })
}

fn gather_packages_for_recipient(
    recipient: &XID,
    all: &HashMap<XID, (ARID, Vec<(XID, frost::keys::dkg::round2::Package)>)>,
) -> Result<Vec<(XID, frost::keys::dkg::round2::Package)>> {
    let mut result = Vec::new();
    for (sender, (_, packages)) in all {
        for (rcpt, pkg) in packages {
            if rcpt == recipient {
                result.push((*sender, pkg.clone()));
            }
        }
    }
    if result.is_empty() {
        bail!(
            "No round2 packages found for recipient {}",
            recipient.ur_string()
        );
    }
    Ok(result)
}

/// Build a finalize request for a participant, including the response ARID
/// where they should post their finalize response.
fn build_finalize_request_for_participant(
    sender: &XIDDocument,
    group_id: &ARID,
    response_arid: ARID,
    packages: &[(XID, frost::keys::dkg::round2::Package)],
) -> Result<SealedRequest> {
    let mut request = SealedRequest::new("dkgFinalize", ARID::new(), sender)
        .with_parameter("group", *group_id)
        .with_parameter("responseArid", response_arid);

    for (pkg_sender, package) in packages {
        let encoded = serde_json::to_vec(package)?;
        let bstr = CBOR::to_byte_string(encoded.as_slice());
        let pkg_envelope =
            Envelope::new(bstr).add_assertion("sender", *pkg_sender);
        request = request.with_parameter("round2Package", pkg_envelope);
    }

    Ok(request)
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

fn group_state_dir(registry_path: &Path, group_id: &ARID) -> PathBuf {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("group-state").join(group_id.hex())
}
