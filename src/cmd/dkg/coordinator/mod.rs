pub mod finalize;
pub mod invite;
pub mod round1;
pub mod round2;

use anyhow::Result;
use clap::{Args, Subcommand};

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
    Invite(invite::CommandArgs),
    /// DKG Round 1 collection and Round 2 dispatch
    Round1(round1::CommandArgs),
    /// DKG Round 2 collection operations
    Round2(round2::CommandArgs),
    /// DKG finalize operations
    Finalize(finalize::CommandArgs),
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
