use anyhow::Result;
use clap::Parser;

use crate::{
    cmd::registry::participants_file_path,
    registry::{OwnerOutcome, OwnerRecord, Registry},
};

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct CommandArgs {
    /// Signed ur:xid document containing the owner's XID document (must
    /// include private keys)
    xid_document: String,
    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let owner = OwnerRecord::from_signed_xid_ur(self.xid_document)?;
        let path = participants_file_path(self.registry)?;
        let mut registry = Registry::load(&path)?;

        match registry.set_owner(owner)? {
            OwnerOutcome::AlreadyPresent => {
                println!("Owner already recorded");
            }
            OwnerOutcome::Inserted => {
                registry.save(&path)?;
                println!("Owner stored in {}", path.display());
            }
        }

        Ok(())
    }
}
