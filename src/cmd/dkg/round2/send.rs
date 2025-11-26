use std::{
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

/// Send Round 2 requests to all participants (coordinator only).
///
/// The coordinator loads the collected Round 1 packages and sends individual
/// sealed messages to each participant so they can generate their Round 2
/// packages.
#[derive(Debug, Parser)]
#[group(skip)]
pub struct CommandArgs {
    #[command(flatten)]
    storage: OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Print the unsealed request envelope UR instead of sending
    #[arg(long = "unsealed")]
    unsealed: bool,

    /// Group ID to send Round 2 requests for
    #[arg(value_name = "GROUP_ID")]
    group_id: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
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
            .context("Registry owner is required")?
            .clone();

        let group_id = parse_arid_ur(&self.group_id)?;
        let group_record = registry
            .group(&group_id)
            .context("Group not found in registry")?
            .clone();

        // Verify we are the coordinator
        if group_record.coordinator().xid() != &owner.xid() {
            bail!(
                "Only the coordinator can send Round 2 requests. \
                 Coordinator: {}, Owner: {}",
                group_record.coordinator().xid().ur_string(),
                owner.xid().ur_string()
            );
        }

        // Load collected Round 1 packages
        let packages_dir = group_state_dir(&registry_path, &group_id);
        let round1_path = packages_dir.join("collected_round1.json");
        if !round1_path.exists() {
            bail!(
                "Round 1 packages not found at {}. \
                 Run 'frost dkg round1 collect' first.",
                round1_path.display()
            );
        }

        let round1_json: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&fs::read(&round1_path).with_context(
                || format!("Failed to read {}", round1_path.display()),
            )?)
            .context("Failed to parse collected_round1.json")?;

        // Parse Round 1 packages
        let round1_packages: Vec<(XID, frost::keys::dkg::round1::Package)> =
            round1_json
                .into_iter()
                .map(|(xid_str, value)| {
                    let xid = XID::from_ur_string(&xid_str)
                        .context("Invalid XID in collected_round1.json")?;
                    let package: frost::keys::dkg::round1::Package =
                        serde_json::from_value(value).context(
                            "Failed to parse Round 1 package from JSON",
                        )?;
                    Ok((xid, package))
                })
                .collect::<Result<Vec<_>>>()?;

        let coordinator_doc = owner.xid_document();
        let signer_private_keys = coordinator_doc
            .inception_private_keys()
            .context("Coordinator XID document has no signing keys")?;
        let valid_until =
            Date::with_duration_from_now(Duration::from_secs(60 * 60));

        // Get pending_requests which contains the send_to ARIDs from Round 1
        // collect. These are where participants told us to post Round 2
        // requests.
        let pending_requests = group_record.pending_requests();
        if pending_requests.is_empty() {
            bail!(
                "No pending requests for this group. \
                 Did you run 'frost dkg round1 collect'?"
            );
        }

        // Build participant info: (XID, XIDDocument, send_to_arid,
        // collect_from_arid) send_to_arid = where we post Round 2
        // request (participant's listening ARID) collect_from_arid =
        // where we'll collect their Round 2 response (new ARID)
        let participant_info: Vec<(XID, XIDDocument, ARID, ARID)> =
            pending_requests
                .iter_send()
                .map(|(xid, send_to_arid)| {
                    let doc = registry
                        .participant(xid)
                        .map(|r| r.xid_document().clone())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Participant {} not found in registry",
                                xid.ur_string()
                            )
                        })?;
                    let collect_from_arid = ARID::new(); // Where we'll collect their Round 2 response
                    Ok((*xid, doc, *send_to_arid, collect_from_arid))
                })
                .collect::<Result<Vec<_>>>()?;

        // Build new pending_requests for Round 2 collection
        let mut new_pending_requests = PendingRequests::new();
        for (xid, _, _, collect_from_arid) in &participant_info {
            new_pending_requests.add_collect_only(*xid, *collect_from_arid);
        }

        if let Some(selection) = selection {
            let runtime = Runtime::new()?;
            let client = runtime.block_on(async {
                StorageClient::from_selection(selection).await
            })?;

            if is_verbose() {
                eprintln!(
                    "Sending Round 2 requests to {} participants...",
                    participant_info.len()
                );
            }

            for (xid, recipient_doc, send_to_arid, collect_from_arid) in
                &participant_info
            {
                let participant_name = registry
                    .participant(xid)
                    .and_then(|r| r.pet_name().map(|s| s.to_owned()))
                    .unwrap_or_else(|| xid.ur_string());

                if is_verbose() {
                    eprint!("  {} ... ", participant_name);
                }

                let request = build_round2_request_for_participant(
                    coordinator_doc,
                    &group_id,
                    &round1_packages,
                    *collect_from_arid, /* Tell participant where to post
                                         * their response */
                )?;

                let sealed_envelope = request.to_envelope_for_recipients(
                    Some(valid_until),
                    Some(signer_private_keys),
                    &[recipient_doc],
                )?;

                // Post to participant's listening ARID
                runtime.block_on(async {
                    client.put(send_to_arid, &sealed_envelope).await
                })?;

                if is_verbose() {
                    eprintln!("ok");
                }
            }

            // Update group record with pending_requests for Round 2 collection
            let group_record = registry
                .group_mut(&group_id)
                .context("Group not found in registry")?;
            group_record.set_pending_requests(new_pending_requests);
            registry.save(&registry_path)?;

            if is_verbose() {
                eprintln!();
                eprintln!("Sent {} Round 2 requests.", participant_info.len());
            }
        } else if self.unsealed {
            // Show a single unsealed request (for preview purposes)
            let (_, _, _, collect_from_arid) = &participant_info[0];
            let request = build_round2_request_for_participant(
                coordinator_doc,
                &group_id,
                &round1_packages,
                *collect_from_arid,
            )?;

            let unsealed_envelope = request.to_envelope(
                Some(valid_until),
                Some(signer_private_keys),
                None, // No recipient = signed but not encrypted
            )?;
            println!("{}", unsealed_envelope.ur_string());
        } else {
            // Sealed but not sent - show each participant's sealed envelope
            for (xid, recipient_doc, _, collect_from_arid) in &participant_info
            {
                let participant_name = registry
                    .participant(xid)
                    .and_then(|r| r.pet_name().map(|s| s.to_owned()))
                    .unwrap_or_else(|| xid.ur_string());

                let request = build_round2_request_for_participant(
                    coordinator_doc,
                    &group_id,
                    &round1_packages,
                    *collect_from_arid,
                )?;

                let sealed_envelope = request.to_envelope_for_recipients(
                    Some(valid_until),
                    Some(signer_private_keys),
                    &[recipient_doc],
                )?;

                eprintln!("# {}", participant_name);
                println!("{}", sealed_envelope.ur_string());
            }
        }

        Ok(())
    }
}

/// Build a Round 2 request for a specific participant.
/// Each participant gets the same Round 1 packages but their own response ARID
/// (where to post their Round 2 response).
fn build_round2_request_for_participant(
    sender: &XIDDocument,
    group_id: &ARID,
    round1_packages: &[(XID, frost::keys::dkg::round1::Package)],
    response_arid: ARID,
) -> Result<SealedRequest> {
    let mut request = SealedRequest::new("dkgRound2", ARID::new(), sender)
        .with_parameter("group", *group_id)
        .with_parameter("responseArid", response_arid);

    // Add all Round 1 packages
    for (xid, package) in round1_packages {
        let encoded = serde_json::to_vec(package)?;
        let bstr = CBOR::to_byte_string(encoded.as_slice());
        let package_envelope =
            Envelope::new(bstr).add_assertion("participant", *xid);
        request = request.with_parameter("round1Package", package_envelope);
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
