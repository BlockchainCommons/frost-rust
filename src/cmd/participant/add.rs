use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::Parser;

use crate::participants::{AddOutcome, ParticipantRecord, ParticipantsFile};

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
        let mut registry = ParticipantsFile::load(&path)?;

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

fn participants_file_path(registry: Option<String>) -> Result<PathBuf> {
    const DEFAULT_FILENAME: &str = "registry.json";
    let cwd = std::env::current_dir()?;

    match registry {
        None => Ok(cwd.join(DEFAULT_FILENAME)),
        Some(raw) => resolve_registry_path(&cwd, DEFAULT_FILENAME, raw),
    }
}

fn resolve_registry_path(
    cwd: &Path,
    default_filename: &str,
    raw: String,
) -> Result<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("Registry path cannot be empty");
    }

    let provided = PathBuf::from(trimmed);
    let treat_as_directory = is_directory_hint(trimmed, &provided);

    let mut resolved = if provided.is_absolute() {
        provided
    } else {
        cwd.join(provided)
    };

    if treat_as_directory {
        resolved.push(default_filename);
    }

    Ok(resolved)
}

fn is_directory_hint(input: &str, path: &Path) -> bool {
    ends_with_separator(input)
        || path.file_name().is_none()
        || matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some(".") | Some("..")
        )
}

fn ends_with_separator(input: &str) -> bool {
    input.ends_with('/') || input.ends_with('\\')
}
