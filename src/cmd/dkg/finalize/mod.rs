pub mod collect;
pub mod respond;
pub mod send;

use anyhow::Result;
use clap::{Args, Subcommand};

/// DKG finalize operations (coordinator).
#[derive(Debug, Args)]
#[group(skip)]
pub struct CoordinatorCommandArgs {
    #[command(subcommand)]
    command: CoordinatorCommands,
}

#[derive(Debug, Subcommand)]
enum CoordinatorCommands {
    /// Send finalize packages to all participants (coordinator only)
    Send(send::CommandArgs),
    /// Respond to finalize packages (participant only)
    /// Collect finalize responses (coordinator only)
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

/// DKG finalize operations (participant).
#[derive(Debug, Args)]
#[group(skip)]
pub struct ParticipantCommandArgs {
    #[command(subcommand)]
    command: ParticipantCommands,
}

#[derive(Debug, Subcommand)]
enum ParticipantCommands {
    /// Respond to finalize packages (participant only)
    Respond(respond::CommandArgs),
}

impl ParticipantCommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            ParticipantCommands::Respond(args) => args.exec(),
        }
    }
}
