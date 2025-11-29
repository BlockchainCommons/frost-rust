use anyhow::Result;
use clap::{Args, Subcommand};

use super::{finalize, invite, round2};

/// Participant-only DKG commands.
#[derive(Debug, Args)]
#[group(skip)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Participant DKG invite operations
    Invite(invite::ParticipantCommandArgs),
    /// DKG Round 2 operations
    Round2(round2::ParticipantCommandArgs),
    /// DKG finalize operations
    Finalize(finalize::ParticipantCommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Invite(args) => args.exec(),
            Commands::Round2(args) => args.exec(),
            Commands::Finalize(args) => args.exec(),
        }
    }
}
