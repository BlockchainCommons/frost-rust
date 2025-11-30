pub mod coordinator;
pub mod participant;

use anyhow::Result;
use clap::{Args, Subcommand};

/// Threshold signing operations.
#[derive(Debug, Args)]
#[group(skip)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Coordinator-only signing commands
    Coordinator(coordinator::CommandArgs),
    /// Participant-only signing commands
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
