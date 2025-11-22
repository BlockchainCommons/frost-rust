use std::{collections::HashSet, time::Duration};

use anyhow::{Result, bail, Context};
use bc_components::{ARID, XID};
use bc_envelope::prelude::*;
use clap::{Parser, Subcommand};
use gstp::SealedRequestBehavior;

use crate::{
    cmd::registry::participants_file_path,
    registry::{ParticipantRecord, Registry},
    DkgGroupInvite,
};

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct CommandArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
#[doc(hidden)]
enum Commands {
    /// Create and display a DKG invite for selected participants
    Invite(InviteArgs),
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            Commands::Invite(args) => args.exec(),
        }
    }
}

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct InviteArgs {
    #[command(subcommand)]
    command: InviteCommands,
}

#[derive(Debug, Subcommand)]
#[doc(hidden)]
enum InviteCommands {
    /// Show a DKG invite for the given participants
    Show(InviteShowArgs),
}

impl InviteArgs {
    pub fn exec(self) -> Result<()> {
        match self.command {
            InviteCommands::Show(args) => args.exec(),
        }
    }
}

#[derive(Debug, Parser)]
#[doc(hidden)]
pub struct InviteShowArgs {
    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Return a sealed invite envelope instead of the request envelope
    #[arg(long)]
    sealed: bool,

    /// Minimum signers required; defaults to participant count
    #[arg(long = "min-signers", value_name = "N")]
    min_signers: Option<usize>,

    /// Charter statement for the DKG session
    #[arg(long = "charter", value_name = "STRING", default_value = "")]
    charter: String,

    /// Participants to include, by pet name or ur:xid identifier
    #[arg(required = true, value_name = "PARTICIPANT")]
    participants: Vec<String>,
}

impl InviteShowArgs {
    pub fn exec(self) -> Result<()> {
        let registry_path = participants_file_path(self.registry.clone())?;
        let registry =
            Registry::load(&registry_path).with_context(|| {
                format!("Failed to load registry at {}", registry_path.display())
            })?;

        let resolved = resolve_participants(&registry, &self.participants)?;
        let participant_docs: Vec<String> = resolved
            .iter()
            .map(|(_, record)| record.xid_document_ur().to_owned())
            .collect();
        let response_arids: Vec<ARID> =
            (0..participant_docs.len()).map(|_| ARID::new()).collect();

        let participant_count = participant_docs.len();
        if participant_count < 2 {
            bail!("At least two participants are required for a DKG invite");
        }
        let min_signers =
            self.min_signers.unwrap_or(participant_count);
        if min_signers < 2 {
            bail!("--min-signers must be at least 2");
        }
        if min_signers > participant_count {
            bail!("--min-signers cannot exceed participant count");
        }

        let invite = DkgGroupInvite::new(
            ARID::new(),
            registry
                .owner()
                .context("Registry owner is required to issue invites")?
                .xid_document()
                .clone(),
            ARID::new(),
            Date::now(),
            Date::with_duration_from_now(Duration::from_secs(60 * 60)),
            min_signers,
            self.charter,
            participant_docs,
            response_arids,
        )?;

        if self.sealed {
            let envelope = invite.to_envelope()?;
            println!("{}", envelope.ur_string());
        } else {
            let envelope = invite.to_request()?.request().to_envelope();
            println!("{}", envelope.ur_string());
        }

        Ok(())
    }
}

fn resolve_participants(
    registry: &Registry,
    inputs: &[String],
) -> Result<Vec<(XID, ParticipantRecord)>> {
    let mut seen_args = HashSet::new();
    let mut seen_xids = HashSet::new();
    let mut resolved = Vec::new();

    for raw in inputs {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            bail!("Participant identifier cannot be empty");
        }
        if !seen_args.insert(trimmed.to_owned()) {
            bail!("Duplicate participant argument: {trimmed}");
        }

        let (xid, record) = if let Ok(xid) = XID::from_ur_string(trimmed) {
            let record = registry.participant(&xid).with_context(|| {
                format!(
                    "Participant with XID {} not found in registry",
                    xid.ur_string()
                )
            })?;
            (xid, record.clone())
        } else {
            let (xid, record) = registry
                .participant_by_pet_name(trimmed)
                .with_context(|| {
                    format!("Participant with pet name '{trimmed}' not found")
                })?;
            (xid.to_owned(), record.clone())
        };

        if !seen_xids.insert(xid) {
            bail!(
                "Duplicate participant specified; multiple inputs resolve to {}",
                xid.ur_string()
            );
        }

        resolved.push((xid, record));
    }

    Ok(resolved)
}
