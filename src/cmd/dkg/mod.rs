mod common;
pub mod invite;

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
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Invite(args) => args.exec(),
        }
    }
}
