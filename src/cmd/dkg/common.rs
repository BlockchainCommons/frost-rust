//! DKG-specific utilities.
//!
//! This module contains utilities specific to DKG operations, such as
//! participant resolution and group participant building.
//!
//! For cross-cutting utilities shared with signing, see [`crate::cmd::common`].

use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use bc_components::{XID, XIDProvider};
use bc_envelope::prelude::*;
use bc_ur::prelude::UR;
use bc_xid::XIDDocument;

// Re-export cross-cutting utilities for convenience
pub use super::super::common::{
    OptionalStorageSelector, group_state_dir, parse_arid_ur,
    signing_key_from_verifying,
};
use crate::registry::{
    GroupParticipant, OwnerRecord, ParticipantRecord, Registry,
};

// -----------------------------------------------------------------------------
// Participant resolution
// -----------------------------------------------------------------------------

pub fn resolve_participants(
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

pub fn resolve_sender(registry: &Registry, input: &str) -> Result<XIDDocument> {
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

pub fn resolve_sender_name(
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

// -----------------------------------------------------------------------------
// Group participant building
// -----------------------------------------------------------------------------

pub fn build_group_participants(
    registry: &Registry,
    owner: &OwnerRecord,
    participants: &[XIDDocument],
) -> Result<Vec<GroupParticipant>> {
    participants
        .iter()
        .map(|doc| group_participant_from_registry(registry, owner, doc))
        .collect()
}

pub fn group_participant_from_registry(
    registry: &Registry,
    owner: &OwnerRecord,
    document: &XIDDocument,
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

// -----------------------------------------------------------------------------
// Name formatting
// -----------------------------------------------------------------------------

pub fn format_name_with_owner_marker(name: String, is_owner: bool) -> String {
    if is_owner { format!("* {name}") } else { name }
}

pub fn participant_names_from_registry(
    registry: &Registry,
    participants: &[XIDDocument],
    owner_xid: &XID,
    owner_pet_name: Option<&str>,
) -> Result<Vec<String>> {
    let mut docs: Vec<XIDDocument> = participants.to_vec();
    docs.sort_by_key(|doc| doc.xid());

    docs.iter()
        .map(|document| {
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
            Ok(format_name_with_owner_marker(name, is_owner))
        })
        .collect()
}

// -----------------------------------------------------------------------------
// Envelope parsing
// -----------------------------------------------------------------------------

pub fn parse_envelope_ur(input: &str) -> Result<Envelope> {
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
