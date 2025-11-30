use std::{collections::BTreeMap, fs};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, JSON, XID, XIDProvider};
use bc_envelope::prelude::*;
use clap::Parser;
use frost_ed25519::{self as frost, Identifier};
use gstp::{
    SealedRequest, SealedRequestBehavior, SealedResponse,
    SealedResponseBehavior,
};
use tokio::runtime::Runtime;

use crate::{
    cmd::{
        dkg::common::{
            OptionalStorageSelector, group_state_dir, parse_arid_ur,
        },
        is_verbose,
        registry::participants_file_path,
        storage::StorageClient,
    },
    registry::Registry,
};

/// Respond to a Round 2 request (participant only).
///
/// Fetches the Round 2 request from Hubert, runs FROST DKG part2 to generate
/// Round 2 packages, and posts the response back to the coordinator.
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

    /// Also print the preview response envelope (no post / no state)
    #[arg(long = "preview")]
    preview: bool,

    /// Group ID to respond to Round 2 for
    #[arg(value_name = "GROUP_ID")]
    group_id: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let selection =
            selection.context("Hubert storage is required for round2")?;

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

        // Get the ARID where we're listening for the Round 2 request
        let listening_at_arid = group_record.listening_at_arid().context(
            "No listening ARID for this group. \
             Did you respond to the invite?",
        )?;

        // Load our Round 1 secret
        let packages_dir = group_state_dir(&registry_path, &group_id);
        let round1_secret_path = packages_dir.join("round1_secret.json");
        if !round1_secret_path.exists() {
            bail!(
                "Round 1 secret not found at {}. \
                 Did you respond to the invite?",
                round1_secret_path.display()
            );
        }
        let round1_secret: frost::keys::dkg::round1::SecretPackage =
            serde_json::from_slice(&fs::read(&round1_secret_path)?)?;

        if is_verbose() {
            eprintln!("Fetching Round 2 request from Hubert...");
        }

        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        // Fetch the Round 2 request from where we're listening
        let request_envelope = runtime.block_on(async {
            client
                .get(&listening_at_arid, self.timeout)
                .await?
                .context("Round 2 request not found in Hubert storage")
        })?;

        // Decrypt and validate the request
        let owner_private_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Owner XID document has no private keys")?;

        let now = Date::now();
        let sealed_request = SealedRequest::try_from_envelope(
            &request_envelope,
            None,
            Some(now),
            owner_private_keys,
        )?;

        // Validate the request
        if sealed_request.function() != &Function::from("dkgRound2") {
            bail!("Unexpected request function: {}", sealed_request.function());
        }

        let expected_coordinator = group_record.coordinator().xid();
        if sealed_request.sender().xid() != *expected_coordinator {
            bail!(
                "Unexpected request sender: {} (expected coordinator {})",
                sealed_request.sender().xid().ur_string(),
                expected_coordinator.ur_string()
            );
        }

        let request_group_id: ARID =
            sealed_request.extract_object_for_parameter("group")?;
        if request_group_id != group_id {
            bail!(
                "Request group ID {} does not match expected {}",
                request_group_id.ur_string(),
                group_id.ur_string()
            );
        }

        // Extract where we should post our response
        let response_arid: ARID =
            sealed_request.extract_object_for_parameter("responseArid")?;

        // Extract Round 1 packages from the request
        let (round1_packages, round1_packages_by_xid) =
            extract_round1_packages(&sealed_request, &group_record, &owner)?;

        if is_verbose() {
            eprintln!(
                "Received {} Round 1 packages. Running DKG part2...",
                round1_packages.len()
            );
        }

        // Allocate next response ARID for the finalize phase
        let next_response_arid = ARID::new();

        // Run FROST DKG part2
        let (round2_secret, round2_packages) =
            frost::keys::dkg::part2(round1_secret, &round1_packages).map_err(
                |e| anyhow::anyhow!("FROST DKG part2 failed: {}", e),
            )?;

        if is_verbose() {
            eprintln!("Generated {} Round 2 packages.", round2_packages.len());
        }

        // Build response with Round 2 packages
        let response_body = build_response_body(
            &group_id,
            &owner.xid(),
            &next_response_arid,
            &round2_packages,
            &group_record,
        )?;

        let signer_private_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Owner XID document has no signing keys")?;

        // Get coordinator's XID document for encryption
        let coordinator_xid = group_record.coordinator().xid();
        let coordinator_doc = registry
            .participant(coordinator_xid)
            .map(|r| r.xid_document().clone())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Coordinator {} not found in registry",
                    coordinator_xid.ur_string()
                )
            })?;

        let sealed_response = SealedResponse::new_success(
            sealed_request.id(),
            owner.xid_document().clone(),
        )
        .with_result(response_body)
        .with_peer_continuation(sealed_request.peer_continuation());

        if self.preview {
            // Show the response envelope structure without encryption
            let unsealed_envelope = sealed_response.to_envelope(
                None, // No expiration for responses
                Some(signer_private_keys),
                None,
            )?;
            println!("{}", unsealed_envelope.ur_string());
            return Ok(());
        }

        // Persist Round 2 secret
        let round2_secret_path = packages_dir.join("round2_secret.json");
        fs::write(
            &round2_secret_path,
            serde_json::to_vec_pretty(&round2_secret)?,
        )?;

        // Persist received Round 1 packages for finalize phase
        let round1_packages_path = packages_dir.join("collected_round1.json");
        let round1_json: serde_json::Map<String, serde_json::Value> =
            round1_packages_by_xid
                .iter()
                .map(|(xid, package)| {
                    (
                        xid.ur_string(),
                        serde_json::to_value(package)
                            .expect("Round1 package serializes"),
                    )
                })
                .collect();
        fs::write(
            &round1_packages_path,
            serde_json::to_vec_pretty(&round1_json)?,
        )?;

        let response_envelope = sealed_response.to_envelope(
            None, // No expiration for responses
            Some(signer_private_keys),
            Some(&coordinator_doc),
        )?;

        // Post the response
        runtime.block_on(async {
            client.put(&response_arid, &response_envelope).await
        })?;

        // Update contributions in registry
        let group_record = registry
            .group_mut(&group_id)
            .context("Group not found in registry")?;
        let mut contributions = group_record.contributions().clone();
        contributions.round2_secret =
            Some(round2_secret_path.to_string_lossy().to_string());
        group_record.set_contributions(contributions);
        // Set new listening ARID for finalize phase
        group_record.set_listening_at_arid(next_response_arid);
        registry.save(&registry_path)?;

        if is_verbose() {
            eprintln!(
                "Posted Round 2 response to {}",
                response_arid.ur_string()
            );
        }

        Ok(())
    }
}

/// Extract Round 1 packages from the request and convert to
/// BTreeMap<Identifier, Package>
fn extract_round1_packages(
    request: &SealedRequest,
    group_record: &crate::registry::GroupRecord,
    owner: &crate::registry::OwnerRecord,
) -> Result<Round1Packages> {
    // Build XID -> Identifier mapping based on sorted participant order
    let mut sorted_xids: Vec<XID> = group_record
        .participants()
        .iter()
        .map(|p| *p.xid())
        .collect();
    if !sorted_xids.contains(&owner.xid()) {
        sorted_xids.push(owner.xid());
    }
    sorted_xids.sort();
    sorted_xids.dedup();

    let xid_to_identifier: std::collections::HashMap<XID, Identifier> =
        sorted_xids
            .iter()
            .enumerate()
            .map(|(i, xid)| {
                let identifier = Identifier::try_from((i + 1) as u16)
                    .expect("Valid identifier");
                (*xid, identifier)
            })
            .collect();

    let my_xid = owner.xid();

    // Extract all round1Package parameters
    let mut packages = BTreeMap::new();
    let mut packages_by_xid = Vec::new();
    for package_envelope in request.objects_for_parameter("round1Package") {
        // Extract participant XID
        let participant_xid: XID =
            package_envelope.extract_object_for_predicate("participant")?;

        // Skip our own package
        if participant_xid == my_xid {
            continue;
        }

        // Extract the package bytes (stored as JSON tag)
        let package_json: bc_components::JSON =
            package_envelope.extract_subject()?;
        let package: frost::keys::dkg::round1::Package =
            serde_json::from_slice(package_json.as_bytes())?;
        let package_for_storage = package.clone();

        // Get the identifier for this participant
        let identifier =
            xid_to_identifier.get(&participant_xid).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown participant XID in round1Package: {}",
                    participant_xid.ur_string()
                )
            })?;

        packages.insert(*identifier, package);
        packages_by_xid.push((participant_xid, package_for_storage));
    }

    let expected_packages = xid_to_identifier.len().saturating_sub(1);
    if packages.len() != expected_packages {
        bail!(
            "Expected {} Round 1 packages, found {}",
            expected_packages,
            packages.len()
        );
    }

    Ok((packages, packages_by_xid))
}

type Round1Packages = (
    BTreeMap<Identifier, frost::keys::dkg::round1::Package>,
    Vec<(XID, frost::keys::dkg::round1::Package)>,
);

/// Build the response body containing Round 2 packages
fn build_response_body(
    group_id: &ARID,
    participant_xid: &XID,
    response_arid: &ARID,
    round2_packages: &BTreeMap<Identifier, frost::keys::dkg::round2::Package>,
    group_record: &crate::registry::GroupRecord,
) -> Result<Envelope> {
    // Build Identifier -> XID mapping
    let mut sorted_xids: Vec<XID> = group_record
        .participants()
        .iter()
        .map(|p| *p.xid())
        .collect();
    if !sorted_xids.contains(participant_xid) {
        sorted_xids.push(*participant_xid);
    }
    sorted_xids.sort();
    sorted_xids.dedup();

    let identifier_to_xid: std::collections::HashMap<Identifier, XID> =
        sorted_xids
            .iter()
            .enumerate()
            .map(|(i, xid)| {
                let identifier = Identifier::try_from((i + 1) as u16)
                    .expect("Valid identifier");
                (identifier, *xid)
            })
            .collect();

    let mut envelope = Envelope::unit()
        .add_type("dkgRound2Response")
        .add_assertion("group", *group_id)
        .add_assertion("participant", *participant_xid)
        .add_assertion("response_arid", *response_arid);

    // Add each Round 2 package with the recipient's XID
    for (identifier, package) in round2_packages {
        let recipient_xid =
            identifier_to_xid.get(identifier).ok_or_else(|| {
                anyhow::anyhow!("Unknown identifier in round2_packages")
            })?;

        let encoded = serde_json::to_vec(package)?;
        let json = JSON::from_data(encoded);
        let package_envelope = Envelope::new(CBOR::from(json))
            .add_assertion("recipient", *recipient_xid);
        envelope = envelope.add_assertion("round2Package", package_envelope);
    }

    Ok(envelope)
}
