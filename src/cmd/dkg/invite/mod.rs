pub mod receive;
pub mod respond;
pub mod send;

use anyhow::Result;
use clap::{Args, Subcommand};

/// Coordinator DKG invite operations.
#[derive(Debug, Args)]
#[group(skip)]
pub struct CoordinatorCommandArgs {
    #[command(subcommand)]
    command: CoordinatorCommands,
}

#[derive(Debug, Subcommand)]
enum CoordinatorCommands {
    /// Compose or send a DKG invite
    Send(send::CommandArgs),
}

impl CoordinatorCommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            CoordinatorCommands::Send(args) => args.exec(),
        }
    }
}

/// Participant DKG invite operations.
#[derive(Debug, Args)]
#[group(skip)]
pub struct ParticipantCommandArgs {
    #[command(subcommand)]
    command: ParticipantCommands,
}

#[derive(Debug, Subcommand)]
enum ParticipantCommands {
    /// Retrieve or inspect a DKG invite
    Receive(receive::CommandArgs),
    /// Respond to a DKG invite
    Respond(respond::CommandArgs),
}

impl ParticipantCommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            ParticipantCommands::Receive(args) => args.exec(),
            ParticipantCommands::Respond(args) => args.exec(),
        }
    }
}
