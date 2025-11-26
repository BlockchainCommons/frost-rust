pub mod collect;

use anyhow::Result;
use clap::{Args, Subcommand};

/// DKG Round 1 operations.
#[derive(Debug, Args)]
#[group(skip)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Collect Round 1 responses from all participants (coordinator only)
    Collect(collect::CommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Collect(args) => args.exec(),
        }
    }
}
