use anyhow::Result;
use clap::Parser;

pub mod cmd;
pub mod registry;

pub use cmd::dkg::{
    DkGProposedParticipant, DkgGroupInvite, DkgInvitation,
    DkgInvitationResult,
};

/// Entrypoint shared by the binary and integration tests.
pub fn run() -> Result<()> {
    bc_components::register_tags();
    bc_envelope::register_tags();
    provenance_mark::register_tags();
    let cli = cmd::Cli::parse();
    cli.exec()
}
