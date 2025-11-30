pub mod invite;
pub mod round1;
pub mod round2;

use anyhow::Result;
use clap::{Args, Subcommand};

/// Coordinator-only signing commands.
#[derive(Debug, Args)]
#[group(skip)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start a threshold signing session (coordinator)
    Invite(invite::CommandArgs),
    /// Collect Round 1 (commit) responses and send Round 2 (share) requests
    Round1(round1::CommandArgs),
    /// Collect Round 2 (share) responses and send finalize packages
    Round2(round2::CommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Invite(args) => args.exec(),
            Commands::Round1(args) => args.exec(),
            Commands::Round2(args) => args.exec(),
        }
    }
}
