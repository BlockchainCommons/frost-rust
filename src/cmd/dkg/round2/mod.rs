pub mod collect;
pub mod respond;
pub mod send;

use anyhow::Result;
use clap::{Args, Subcommand};

/// DKG Round 2 operations (coordinator).
#[derive(Debug, Args)]
#[group(skip)]
pub struct CoordinatorCommandArgs {
    #[command(subcommand)]
    command: CoordinatorCommands,
}

#[derive(Debug, Subcommand)]
enum CoordinatorCommands {
    /// Send Round 2 request to all participants (coordinator only)
    Send(send::CommandArgs),
    /// Respond to a Round 2 request (participant only)
    /// Collect Round 2 responses from all participants (coordinator only)
    Collect(collect::CommandArgs),
}

impl CoordinatorCommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            CoordinatorCommands::Send(args) => args.exec(),
            CoordinatorCommands::Collect(args) => args.exec(),
        }
    }
}

/// DKG Round 2 operations (participant).
#[derive(Debug, Args)]
#[group(skip)]
pub struct ParticipantCommandArgs {
    #[command(subcommand)]
    command: ParticipantCommands,
}

#[derive(Debug, Subcommand)]
enum ParticipantCommands {
    /// Respond to a Round 2 request (participant only)
    Respond(respond::CommandArgs),
}

impl ParticipantCommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            ParticipantCommands::Respond(args) => args.exec(),
        }
    }
}
