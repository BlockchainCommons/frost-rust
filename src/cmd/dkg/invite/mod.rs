pub mod receive;
pub mod respond;
pub mod send;

use anyhow::Result;
use clap::{Args, Subcommand};

use super::common;

/// DKG invite operations.
#[derive(Debug, Args)]
#[group(skip)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Compose or send a DKG invite
    Send(send::CommandArgs),
    /// Retrieve or inspect a DKG invite
    Receive(receive::CommandArgs),
    /// Respond to a DKG invite
    Respond(respond::CommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Send(args) => args.exec(),
            Commands::Receive(args) => args.exec(),
            Commands::Respond(args) => args.exec(),
        }
    }
}
