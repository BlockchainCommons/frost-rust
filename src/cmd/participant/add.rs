use anyhow::{Result, bail};
use clap::Parser;

use crate::{
    cmd::registry::participants_file_path,
    registry::{AddOutcome, ParticipantRecord, Registry},
};

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct CommandArgs {
    /// Signed ur:xid document containing the participant's XID document
    xid_document: String,
    /// Optional human readable alias
    pet_name: Option<String>,
    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let pet_name = normalize_pet_name(self.pet_name)?;
        let participant =
            ParticipantRecord::from_signed_xid_ur(self.xid_document, pet_name)?;
        let xid = participant.xid();
        let path = participants_file_path(self.registry)?;
        let mut registry = Registry::load(&path)?;

        match registry.add(xid, participant)? {
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
