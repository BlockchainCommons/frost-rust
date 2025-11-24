use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

#[doc(hidden)]
mod owner;
#[doc(hidden)]
mod participant;

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
#[doc(hidden)]
enum Commands {
    /// Manage registry participants
    Participant(participant::CommandArgs),
    /// Manage the registry owner
    Owner(owner::CommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Participant(args) => args.exec(),
            Commands::Owner(args) => args.exec(),
        }
    }
}

/// Resolve the participants registry path, defaulting to `registry.json` in the
/// current working directory.
pub fn participants_file_path(registry: Option<String>) -> Result<PathBuf> {
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
