pub mod respond;
pub mod send;

use anyhow::Result;
use clap::{Args, Subcommand};

/// DKG Round 2 operations.
#[derive(Debug, Args)]
#[group(skip)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Send Round 2 request to all participants (coordinator only)
    Send(send::CommandArgs),
    /// Respond to a Round 2 request (participant only)
    Respond(respond::CommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Send(args) => args.exec(),
            Commands::Respond(args) => args.exec(),
        }
    }
}
