mod common;
pub mod invite;
pub mod round1;

use anyhow::Result;
use clap::{Args, Subcommand};

/// Distributed key generation operations.
#[derive(Debug, Args)]
#[group(skip)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create, receive, or respond to DKG invites
    Invite(invite::CommandArgs),
    /// DKG Round 1 operations
    Round1(round1::CommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Invite(args) => args.exec(),
            Commands::Round1(args) => args.exec(),
        }
    }
}
