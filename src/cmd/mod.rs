use anyhow::Result;
use clap::{Parser, Subcommand};

#[doc(hidden)]
pub mod participant;

/// FROST command-line interface definition.
#[derive(Debug, Parser)]
#[command(author, version, about = "FROST command line toolkit")]
#[doc(hidden)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
#[doc(hidden)]
enum Commands {
    /// Manage FROST participants
    Participant(participant::CommandArgs),
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Participant(args) => args.exec(),
        }
    }
}
