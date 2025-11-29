use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use bc_components::{ARID, Ed25519PublicKey, JSON, SigningPublicKey, XID};
use bc_envelope::prelude::*;
use clap::Parser;
use gstp::SealedResponse;
use tokio::runtime::Runtime;

use super::super::common::OptionalStorageSelector;
use crate::{
    cmd::{
        is_verbose, registry::participants_file_path, storage::StorageClient,
    },
    registry::Registry,
};

/// Collect finalize responses (key/public key packages) from all participants
/// (coordinator only).
#[derive(Debug, Parser)]
#[group(skip)]
pub struct CommandArgs {
    #[command(flatten)]
    storage: OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Wait up to this many seconds for each response to appear
    #[arg(long = "timeout", value_name = "SECONDS")]
    timeout: Option<u64>,

    /// Group ID to collect finalize responses for
    #[arg(value_name = "GROUP_ID")]
    group_id: String,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        let selection = selection
            .context("Hubert storage is required for finalize collect")?;

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

        if group_record.coordinator().xid() != &owner.xid() {
            bail!(
                "Only the coordinator can collect finalize responses. Coordinator: {}, Owner: {}",
                group_record.coordinator().xid().ur_string(),
                owner.xid().ur_string()
            );
        }

        let pending_requests = group_record.pending_requests();
        if pending_requests.is_empty() {
            bail!(
                "No pending requests for this group. \
                 Did you run 'frost dkg coordinator finalize send'?"
            );
        }

        let runtime = Runtime::new()?;
        let client = runtime.block_on(async {
            StorageClient::from_selection(selection).await
        })?;

        let coordinator_keys = owner
            .xid_document()
            .inception_private_keys()
            .context("Coordinator XID document has no private keys")?;

        let mut collected: Vec<FinalizeEntry> = Vec::new();
        let mut errors: Vec<(XID, String)> = Vec::new();
        let mut group_verifying_key: Option<SigningPublicKey> = None;

        if is_verbose() {
            eprintln!(
                "Collecting finalize responses from {} participants...",
                pending_requests.len()
            );
        }

        for (participant_xid, collect_from_arid) in
            pending_requests.iter_collect()
        {
            let name = registry
                .participant(participant_xid)
                .and_then(|r| r.pet_name().map(|s| s.to_owned()))
                .unwrap_or_else(|| participant_xid.ur_string());
            if is_verbose() {
                eprintln!("{}...", name);
            }

            match fetch_finalize_response(
                &runtime,
                &client,
                collect_from_arid,
                self.timeout,
                coordinator_keys,
                &group_id,
                participant_xid,
            ) {
                Ok(entry) => match signing_key_from_verifying(
                    entry.public_key_package.verifying_key(),
                ) {
                    Ok(signing_key) => {
                        if let Some(existing) = &group_verifying_key {
                            if existing != &signing_key {
                                if is_verbose() {
                                    eprintln!(
                                        "error: group verifying key mismatch"
                                    );
                                }
                                errors.push((
                                    *participant_xid,
                                    "Group verifying key mismatch across responses"
                                        .to_string(),
                                ));
                                continue;
                            }
                        } else {
                            group_verifying_key = Some(signing_key);
                        }

                        collected.push(entry);
                    }
                    Err(err) => {
                        if is_verbose() {
                            eprintln!("error: {}", err);
                        }
                        errors.push((*participant_xid, err.to_string()));
                    }
                },
                Err(err) => {
                    if is_verbose() {
                        eprintln!("error: {}", err);
                    }
                    errors.push((*participant_xid, err.to_string()));
                }
            }
        }

        if !errors.is_empty() {
            if is_verbose() {
                eprintln!();
                eprintln!(
                    "Failed to collect from {} participants:",
                    errors.len()
                );
                for (xid, error) in &errors {
                    eprintln!("  {}: {}", xid.ur_string(), error);
                }
            }
            bail!(
                "Finalize collection incomplete: {} of {} responses failed",
                errors.len(),
                pending_requests.len()
            );
        }

        // Persist collected finalize data
        let state_dir = group_state_dir(&registry_path, &group_id);
        fs::create_dir_all(&state_dir).with_context(|| {
            format!(
                "Failed to create group state directory {}",
                state_dir.display()
            )
        })?;

        let collected_path = state_dir.join("collected_finalize.json");
        let mut root = serde_json::Map::new();
        for entry in &collected {
            let mut m = serde_json::Map::new();
            m.insert(
                "key_package".to_string(),
                serde_json::to_value(&entry.key_package)
                    .expect("key_package JSON"),
            );
            m.insert(
                "public_key_package".to_string(),
                serde_json::to_value(&entry.public_key_package)
                    .expect("public key JSON"),
            );
            root.insert(
                entry.participant.ur_string(),
                serde_json::Value::Object(m),
            );
        }
        fs::write(&collected_path, serde_json::to_vec_pretty(&root)?)
            .with_context(|| {
                format!("Failed to write {}", collected_path.display())
            })?;

        // Update registry pending requests cleared
        let group_record = registry
            .group_mut(&group_id)
            .context("Group not found in registry")?;
        if let Some(key) = &group_verifying_key {
            group_record.set_verifying_key(key.clone());
        }
        group_record.clear_pending_requests();
        registry.save(&registry_path)?;

        if is_verbose() {
            eprintln!();
            eprintln!(
                "Collected {} finalize responses. Saved to {}",
                collected.len(),
                collected_path.display()
            );
            if let Some(key) = group_verifying_key {
                eprintln!("{}", key.ur_string());
            }
        } else {
            println!("{}", collected_path.display());
            if let Some(key) = group_verifying_key {
                println!("{}", key.ur_string());
            }
        }

        Ok(())
    }
}

struct FinalizeEntry {
    participant: XID,
    key_package: frost_ed25519::keys::KeyPackage,
    public_key_package: frost_ed25519::keys::PublicKeyPackage,
}

fn signing_key_from_verifying(
    verifying_key: &frost_ed25519::VerifyingKey,
) -> Result<SigningPublicKey> {
    let bytes = verifying_key
        .serialize()
        .map_err(|e| anyhow!("Failed to serialize verifying key: {e}"))?;
    let ed25519 = Ed25519PublicKey::from_data_ref(bytes)
        .context("Group verifying key is not a valid Ed25519 public key")?;
    Ok(SigningPublicKey::from_ed25519(ed25519))
}

fn fetch_finalize_response(
    runtime: &Runtime,
    client: &StorageClient,
    response_arid: &ARID,
    timeout: Option<u64>,
    coordinator_keys: &bc_components::PrivateKeys,
    expected_group: &ARID,
    expected_participant: &XID,
) -> Result<FinalizeEntry> {
    let envelope = runtime.block_on(async {
        client
            .get(response_arid, timeout)
            .await?
            .context("Finalize response not found in Hubert storage")
    })?;

    let now = Date::now();
    let sealed = SealedResponse::try_from_encrypted_envelope(
        &envelope,
        None,
        Some(now),
        coordinator_keys,
    )?;

    if let Ok(error) = sealed.error() {
        let reason = error
            .object_for_predicate("reason")
            .ok()
            .and_then(|e| e.extract_subject::<String>().ok())
            .unwrap_or_else(|| "unknown reason".to_string());
        bail!("Participant reported error: {}", reason);
    }

    let result = sealed.result().context("Finalize response has no result")?;
    let function: String = result.extract_subject()?;
    if function != "dkgFinalizeResponse" {
        bail!("Unexpected response function: {}", function);
    }

    let group_id: ARID = result.extract_object_for_predicate("group")?;
    if &group_id != expected_group {
        bail!(
            "Group {} does not match expected {}",
            group_id.ur_string(),
            expected_group.ur_string()
        );
    }

    let participant_xid: XID =
        result.extract_object_for_predicate("participant")?;
    if &participant_xid != expected_participant {
        bail!(
            "Participant {} does not match expected {}",
            participant_xid.ur_string(),
            expected_participant.ur_string()
        );
    }

    let key_json: JSON = result.extract_object_for_predicate("key_package")?;
    let pub_json: JSON =
        result.extract_object_for_predicate("public_key_package")?;

    let key_package: frost_ed25519::keys::KeyPackage =
        serde_json::from_slice(key_json.as_bytes())
            .context("Failed to parse key_package")?;
    let public_key_package: frost_ed25519::keys::PublicKeyPackage =
        serde_json::from_slice(pub_json.as_bytes())
            .context("Failed to parse public_key_package")?;

    Ok(FinalizeEntry {
        participant: participant_xid,
        key_package,
        public_key_package,
    })
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
