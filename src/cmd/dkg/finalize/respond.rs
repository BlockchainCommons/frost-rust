use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use bc_components::{
    ARID, Ed25519PublicKey, JSON, SigningPublicKey, XID, XIDProvider,
};
use bc_envelope::prelude::*;
use clap::Parser;
use frost_ed25519 as frost;
use gstp::{
    SealedRequest, SealedRequestBehavior, SealedResponse,
    SealedResponseBehavior,
};
use tokio::runtime::Runtime;

use super::super::common::{OptionalStorageSelector, parse_arid_ur};
use crate::{
    cmd::{
        is_verbose, registry::participants_file_path, storage::StorageClient,
    },
    registry::Registry,
};

/// Respond to finalize request (participant only).
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

    /// Group ID to respond for
    #[arg(value_name = "GROUP_ID")]
    group_id: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let selection = selection
            .context("Hubert storage is required for finalize respond")?;

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

        let listening_at_arid = group_record.listening_at_arid().context(
            "No listening ARID for this group. Did you receive finalize send?",
        )?;

        // Load Round 2 secret
        let state_dir = group_state_dir(&registry_path, &group_id);
        let round2_secret_path = state_dir.join("round2_secret.json");
        if !round2_secret_path.exists() {
            bail!(
                "Round 2 secret not found at {}. Did you run round2 respond?",
                round2_secret_path.display()
            );
        }
        let round2_secret: frost::keys::dkg::round2::SecretPackage =
            serde_json::from_slice(&fs::read(&round2_secret_path)?)?;

        // Load collected Round 1 packages (from earlier phases)
        let round1_path = state_dir.join("collected_round1.json");
        if !round1_path.exists() {
            bail!(
                "Round 1 packages not found at {}. Did you receive earlier phases?",
                round1_path.display()
            );
        }
        let round1_json: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&fs::read(&round1_path).with_context(
                || format!("Failed to read {}", round1_path.display()),
            )?)
            .context("Failed to parse collected_round1.json")?;

        if is_verbose() {
            eprintln!("Fetching finalize request from Hubert...");
        }

        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        let request_envelope = runtime.block_on(async {
            client
                .get(&listening_at_arid, self.timeout)
                .await?
                .context("Finalize request not found in Hubert storage")
        })?;

        let owner_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Owner XID document has no private keys")?;

        let now = Date::now();
        let sealed_request = SealedRequest::try_from_envelope(
            &request_envelope,
            None,
            Some(now),
            owner_keys,
        )?;

        if sealed_request.function() != &Function::from("dkgFinalize") {
            bail!("Unexpected request function: {}", sealed_request.function());
        }

        // Verify coordinator sender
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

        let response_arid: ARID =
            sealed_request.extract_object_for_parameter("responseArid")?;

        // Build identifier mapping
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

        let xid_to_id: std::collections::HashMap<XID, frost::Identifier> =
            sorted_xids
                .iter()
                .enumerate()
                .map(|(i, xid)| {
                    let id =
                        frost::Identifier::try_from((i + 1) as u16).unwrap();
                    (*xid, id)
                })
                .collect();

        // Round1 packages map (exclude self)
        let mut round1_map: BTreeMap<
            frost::Identifier,
            frost::keys::dkg::round1::Package,
        > = BTreeMap::new();
        for (xid_str, value) in &round1_json {
            let xid = XID::from_ur_string(xid_str)
                .context("Invalid XID in collected_round1.json")?;
            if xid == owner.xid() {
                continue;
            }
            let id = xid_to_id.get(&xid).ok_or_else(|| {
                anyhow::anyhow!("Unknown participant XID {}", xid.ur_string())
            })?;
            let pkg: frost::keys::dkg::round1::Package =
                serde_json::from_value(value.clone())
                    .context("Failed to parse Round 1 package")?;
            round1_map.insert(*id, pkg);
        }

        // Round2 packages extracted from request (exclude self)
        let mut round2_map: BTreeMap<
            frost::Identifier,
            frost::keys::dkg::round2::Package,
        > = BTreeMap::new();
        for pkg_env in sealed_request.objects_for_parameter("round2Package") {
            let sender_xid: XID =
                pkg_env.extract_object_for_predicate("sender")?;
            if sender_xid == owner.xid() {
                continue;
            }
            let id = xid_to_id.get(&sender_xid).ok_or_else(|| {
                anyhow::anyhow!("Unknown sender XID {}", sender_xid.ur_string())
            })?;
            let pkg_json: JSON =
                pkg_env.extract_subject().context("round2Package missing")?;
            let pkg: frost::keys::dkg::round2::Package =
                serde_json::from_slice(pkg_json.as_bytes())
                    .context("Failed to deserialize round2 package")?;
            round2_map.insert(*id, pkg);
        }

        if is_verbose() {
            eprintln!(
                "Received {} Round 2 packages. Running DKG part3...",
                round2_map.len()
            );
        }

        let (key_package, public_key_package) =
            frost::keys::dkg::part3(&round2_secret, &round1_map, &round2_map)
                .map_err(|e| anyhow::anyhow!("FROST DKG part3 failed: {}", e))?;

        let group_verifying_key =
            signing_key_from_verifying(public_key_package.verifying_key())
                .context("Failed to derive group verifying key")?;

        if is_verbose() {
            eprintln!("Generated key package and public key package.");
        }

        // Persist key packages
        let key_package_path = state_dir.join("key_package.json");
        let public_key_package_path = state_dir.join("public_key_package.json");
        fs::write(&key_package_path, serde_json::to_vec_pretty(&key_package)?)?;
        fs::write(
            &public_key_package_path,
            serde_json::to_vec_pretty(&public_key_package)?,
        )?;

        // Build response
        let response_body = build_response_body(
            &group_id,
            &owner.xid(),
            &key_package,
            &public_key_package,
        )?;

        let signer_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Owner XID document has no signing keys")?;

        // Coordinator doc
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
            if is_verbose() {
                eprintln!("{}", group_verifying_key.ur_string());
            }
            let unsealed_envelope =
                sealed_response.to_envelope(None, Some(signer_keys), None)?;
            println!("{}", unsealed_envelope.ur_string());
            return Ok(());
        }

        let response_envelope = sealed_response.to_envelope(
            None,
            Some(signer_keys),
            Some(&coordinator_doc),
        )?;

        runtime.block_on(async {
            client.put(&response_arid, &response_envelope).await
        })?;

        // Update registry contributions
        let group_record = registry
            .group_mut(&group_id)
            .context("Group not found in registry")?;
        let mut contributions = group_record.contributions().clone();
        contributions.key_package =
            Some(key_package_path.to_string_lossy().into_owned());
        group_record.set_contributions(contributions);
        group_record.clear_listening_at_arid();
        group_record.set_verifying_key(group_verifying_key);
        let group_key = group_record.verifying_key().cloned();
        registry.save(&registry_path)?;

        if is_verbose() {
            eprintln!(
                "Posted finalize response to {}",
                response_arid.ur_string()
            );
            if let Some(key) = group_key.as_ref() {
                eprintln!("{}", key.ur_string());
            }
        } else if let Some(key) = group_key.as_ref() {
            println!("{}", key.ur_string());
        }

        Ok(())
    }
}

fn build_response_body(
    group_id: &ARID,
    participant: &XID,
    key_package: &frost_ed25519::keys::KeyPackage,
    public_key_package: &frost_ed25519::keys::PublicKeyPackage,
) -> Result<Envelope> {
    let key_json = JSON::from_data(serde_json::to_vec(key_package)?);
    let pub_json = JSON::from_data(serde_json::to_vec(public_key_package)?);

    Ok(Envelope::new("dkgFinalizeResponse")
        .add_assertion("group", *group_id)
        .add_assertion("participant", *participant)
        .add_assertion("key_package", CBOR::from(key_json))
        .add_assertion("public_key_package", CBOR::from(pub_json)))
}

fn signing_key_from_verifying(
    verifying_key: &frost_ed25519::VerifyingKey,
) -> Result<SigningPublicKey> {
    let bytes = verifying_key.serialize().map_err(|e| {
        anyhow::anyhow!("Failed to serialize verifying key: {e}")
    })?;
    let ed25519 = Ed25519PublicKey::from_data_ref(bytes)
        .context("Group verifying key is not a valid Ed25519 public key")?;
    Ok(SigningPublicKey::from_ed25519(ed25519))
}

fn group_state_dir(registry_path: &Path, group_id: &ARID) -> PathBuf {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("group-state").join(group_id.hex())
}
