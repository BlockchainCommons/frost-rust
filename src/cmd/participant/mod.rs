use anyhow::Result;
use clap::{Parser, Subcommand};

#[doc(hidden)]
mod add;

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
#[doc(hidden)]
enum Commands {
    /// Add a participant using an ur:xid document
    Add(add::CommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Add(args) => args.exec(),
        }
    }
}
