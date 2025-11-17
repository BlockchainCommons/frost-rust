use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use bc_envelope::prelude::*;
use bc_xid::{XIDDocument, XIDVerifySignature};
use clap::Parser;

use crate::participants::{AddOutcome, ParticipantRecord, ParticipantsFile};

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct CommandArgs {
    /// Signed ur:xid document containing the participant's XID document
    xid_document: String,
    /// Optional human readable alias
    pet_name: Option<String>,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let pet_name = normalize_pet_name(self.pet_name)?;
        let envelope = parse_xid_envelope(&self.xid_document)?;
        let document = XIDDocument::from_envelope(
            &envelope,
            None,
            XIDVerifySignature::Inception,
        )
        .context("XID document must be signed by its inception key")?;

        let participant =
            ParticipantRecord::from_sources(&document, &envelope, pet_name)?;
        let path = participants_file_path()?;
        let mut registry = ParticipantsFile::load(&path)?;

        match registry.add(participant)? {
            AddOutcome::AlreadyPresent => {
                println!("Participant already recorded");
            }
            AddOutcome::Inserted => {
                registry.save(&path)?;
                println!("Participant stored in {}", path.display());
            }
        }

        Ok(())
    }
}

fn parse_xid_envelope(input: &str) -> Result<Envelope> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("XID document is required");
    }

    let ur = UR::from_ur_string(trimmed)
        .with_context(|| format!("Failed to parse UR: {trimmed}"))?;
    if ur.ur_type_str() != "xid" {
        bail!("Expected a ur:xid document, found ur:{}", ur.ur_type_str());
    }

    Envelope::from_tagged_cbor(ur.cbor())
        .context("Unable to decode XID document envelope")
}

fn normalize_pet_name(pet_name: Option<String>) -> Result<Option<String>> {
    match pet_name {
        None => Ok(None),
        Some(name) => {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                bail!("Pet name cannot be empty");
            }
            Ok(Some(trimmed.to_owned()))
        }
    }
}

fn participants_file_path() -> Result<PathBuf> {
    Ok(std::env::current_dir()?.join("particiapants.json"))
}
