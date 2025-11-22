use anyhow::Result;
use clap::{Parser, Subcommand};

#[doc(hidden)]
pub mod participant;
#[doc(hidden)]
pub mod owner;
#[doc(hidden)]
pub mod dkg;
#[doc(hidden)]
pub mod registry;
#[doc(hidden)]
pub mod storage;
#[doc(hidden)]
pub mod check;

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
    /// Manage FROST registry owner
    Owner(owner::CommandArgs),
    /// Check Hubert storage backend availability
    Check(check::CommandArgs),
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Participant(args) => args.exec(),
            Commands::Owner(args) => args.exec(),
            Commands::Check(args) => args.exec(),
        }
    }
}
