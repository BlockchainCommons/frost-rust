use anyhow::Result;
use clap::{Parser, Subcommand};

#[doc(hidden)]
pub mod dkg;
#[doc(hidden)]
pub mod registry;
#[doc(hidden)]
pub mod dkg_cli;
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
    /// Manage the FROST registry
    Registry(registry::CommandArgs),
    /// Check Hubert storage backend availability
    Check(check::CommandArgs),
    /// Distributed key generation operations
    Dkg(dkg_cli::CommandArgs),
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
