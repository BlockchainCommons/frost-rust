use anyhow::Result;
use clap::Parser;

pub mod cmd;
pub mod participants;

pub use cmd::dkg::{DkGProposedParticipant, DkgGroupInvite, DkgInvitation};

/// Entrypoint shared by the binary and integration tests.
pub fn run() -> Result<()> {
    bc_envelope::register_tags();
    let cli = cmd::Cli::parse();
    cli.exec()
}
