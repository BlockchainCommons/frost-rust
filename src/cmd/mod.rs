use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use clap::{Parser, Subcommand};

pub mod check;
pub mod common;
pub mod dkg;
pub mod parallel;
pub mod registry;
pub mod sign;
pub mod storage;

/// FROST command-line interface definition.
#[derive(Debug, Parser)]
#[command(author, version, about = "FROST command line toolkit")]
#[command(infer_subcommands = true)]
pub struct Cli {
    /// Enable verbose output for hubert interactions and progress messages
    #[arg(long, global = true)]
    verbose: bool,

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
    /// Threshold signing operations
    Sign(sign::CommandArgs),
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        set_verbose(self.verbose);
        match self.command {
            Commands::Registry(args) => args.exec(),
            Commands::Check(args) => args.exec(),
            Commands::Dkg(args) => args.exec(),
            Commands::Sign(args) => args.exec(),
        }
    }
}

static VERBOSE: AtomicBool = AtomicBool::new(false);

pub fn set_verbose(value: bool) { VERBOSE.store(value, Ordering::Relaxed); }

pub fn is_verbose() -> bool { VERBOSE.load(Ordering::Relaxed) }
