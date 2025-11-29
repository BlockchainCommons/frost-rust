use anyhow::Result;
use clap::{Args, Subcommand};

use super::{finalize, invite, round1, round2};

/// Coordinator-only DKG commands.
#[derive(Debug, Args)]
#[group(skip)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Coordinator DKG invite operations
    Invite(invite::CoordinatorCommandArgs),
    /// DKG Round 1 operations
    Round1(round1::CoordinatorCommandArgs),
    /// DKG Round 2 operations
    Round2(round2::CoordinatorCommandArgs),
    /// DKG finalize operations
    Finalize(finalize::CoordinatorCommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Invite(args) => args.exec(),
            Commands::Round1(args) => args.exec(),
            Commands::Round2(args) => args.exec(),
            Commands::Finalize(args) => args.exec(),
        }
    }
}
