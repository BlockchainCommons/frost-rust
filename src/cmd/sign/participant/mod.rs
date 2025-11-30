pub mod finalize;
pub mod receive;
pub mod round1;
pub mod round2;

use anyhow::Result;
use clap::{Args, Subcommand};

/// Participant-only signing commands.
#[derive(Debug, Args)]
#[group(skip)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Inspect a signing invite (participant)
    Receive(receive::CommandArgs),
    /// Respond to a signing invite with commitments (Round 1)
    Round1(round1::CommandArgs),
    /// Respond with signature share (Round 2)
    Round2(round2::CommandArgs),
    /// Attach finalized signature to target envelope (participant)
    Finalize(finalize::CommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Receive(args) => args.exec(),
            Commands::Round1(args) => args.exec(),
            Commands::Round2(args) => args.exec(),
            Commands::Finalize(args) => args.exec(),
        }
    }
}
