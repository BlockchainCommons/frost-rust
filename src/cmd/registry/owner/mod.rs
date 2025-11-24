use anyhow::Result;
use clap::{Parser, Subcommand};

mod set;

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
#[doc(hidden)]
enum Commands {
    /// Set the registry owner using an ur:xid document that includes private keys
    Set(set::CommandArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Set(args) => args.exec(),
        }
    }
}
