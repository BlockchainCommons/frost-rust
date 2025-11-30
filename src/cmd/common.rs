//! Cross-cutting utilities shared by DKG and signing subcommands.
//!
//! This module contains utilities that are needed by both the `dkg` and `sign`
//! subcommand hierarchies, including:
//! - ARID/UR parsing
//! - Storage backend selection
//! - Verifying key conversion
//! - Group state directory helpers

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use bc_components::{ARID, Ed25519PublicKey, SigningPublicKey};
use bc_envelope::prelude::CBOR;
use bc_ur::prelude::UR;
use clap::Args;

use super::storage::{StorageBackend, StorageSelection, StorageSelector};

// -----------------------------------------------------------------------------
// ARID/UR parsing
// -----------------------------------------------------------------------------

/// Parses a `ur:arid` string into an [`ARID`].
///
/// Accepts a trimmed UR string and validates that it is of type `arid`.
pub fn parse_arid_ur(input: &str) -> Result<ARID> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("ARID is required");
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

// -----------------------------------------------------------------------------
// Storage selection
// -----------------------------------------------------------------------------

/// Optional storage backend selection for commands that can work with or
/// without Hubert.
#[derive(Debug, Clone, Args)]
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

// -----------------------------------------------------------------------------
// Verifying key conversion
// -----------------------------------------------------------------------------

/// Converts a FROST Ed25519 verifying key to a [`SigningPublicKey`].
pub fn signing_key_from_verifying(
    verifying_key: &frost_ed25519::VerifyingKey,
) -> Result<SigningPublicKey> {
    let bytes = verifying_key
        .serialize()
        .map_err(|e| anyhow!("Failed to serialize verifying key: {e}"))?;
    let ed25519 = Ed25519PublicKey::from_data_ref(bytes)
        .context("Group verifying key is not a valid Ed25519 public key")?;
    Ok(SigningPublicKey::from_ed25519(ed25519))
}

// -----------------------------------------------------------------------------
// Group state directory
// -----------------------------------------------------------------------------

/// Returns the group state directory for a DKG group.
///
/// Path: `{registry_dir}/group-state/{group_id.hex()}`
pub fn group_state_dir(registry_path: &Path, group_id: &ARID) -> PathBuf {
    let base = registry_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("group-state").join(group_id.hex())
}
