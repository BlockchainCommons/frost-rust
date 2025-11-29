pub mod common;
pub mod coordinator;
pub mod finalize;
pub mod invite;
pub mod participant;
pub mod round1;
pub mod round2;

use anyhow::Result;
use clap::{Args, Subcommand};
pub use common::OptionalStorageSelector;

/// Distributed key generation operations.
#[derive(Debug, Args)]
#[group(skip)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Coordinator-only DKG commands
    Coordinator(coordinator::CommandArgs),
    /// Participant-only DKG commands
    Participant(participant::CommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Coordinator(args) => args.exec(),
            Commands::Participant(args) => args.exec(),
        }
    }
}
