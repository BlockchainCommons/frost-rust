pub mod collect;
pub mod finalize;
pub mod start;

use anyhow::Result;
use clap::{Args, Subcommand};

/// Coordinator-only signing commands.
#[derive(Debug, Args)]
#[group(skip)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start a threshold signing session (coordinator only)
    Start(start::CommandArgs),
    /// Collect signCommit responses and send signShare requests (coordinator)
    Collect(collect::CommandArgs),
    /// Collect signature shares and fan out finalize packages (coordinator)
    Finalize(finalize::CommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Start(args) => args.exec(),
            Commands::Collect(args) => args.exec(),
            Commands::Finalize(args) => args.exec(),
        }
    }
}
