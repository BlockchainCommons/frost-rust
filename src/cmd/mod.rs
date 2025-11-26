use anyhow::Result;
use clap::{Parser, Subcommand};

pub mod check;
pub mod dkg;
pub mod registry;
pub mod storage;

/// FROST command-line interface definition.
#[derive(Debug, Parser)]
#[command(author, version, about = "FROST command line toolkit")]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Manage the FROST registry
    Registry(registry::CommandArgs),
    /// Check Hubert storage backend availability
    Check(check::CommandArgs),
    /// Distributed key generation operations
    Dkg(dkg::CommandArgs),
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Registry(args) => args.exec(),
            Commands::Check(args) => args.exec(),
            Commands::Dkg(args) => args.exec(),
        }
    }
}
