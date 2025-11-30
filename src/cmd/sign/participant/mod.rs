pub mod attach;
pub mod commit;
pub mod receive;
pub mod share;

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
    /// Inspect a signCommit request (participant)
    Receive(receive::CommandArgs),
    /// Respond to a signCommit request (participant)
    Commit(commit::CommandArgs),
    /// Respond to a signShare request with a signature share (participant)
    Share(share::CommandArgs),
    /// Attach a finalized signature to the target envelope (participant)
    Attach(attach::CommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Receive(args) => args.exec(),
            Commands::Commit(args) => args.exec(),
            Commands::Share(args) => args.exec(),
            Commands::Attach(args) => args.exec(),
        }
    }
}
