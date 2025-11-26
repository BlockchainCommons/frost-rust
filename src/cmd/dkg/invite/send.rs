use std::time::Duration;

use anyhow::{Context, Result, bail};
use bc_components::ARID;
use bc_envelope::prelude::*;
use clap::Parser;
use tokio::runtime::Runtime;

use super::common::{OptionalStorageSelector, resolve_participants};
use crate::{
    DkgGroupInvite,
    cmd::{registry::participants_file_path, storage::StorageClient},
    registry::Registry,
};

/// Compose or send a DKG invite.
#[derive(Debug, Parser)]
#[group(skip)]
pub struct CommandArgs {
    #[command(flatten)]
    storage: OptionalStorageSelector,

    /// Optional registry path or filename override
    #[arg(long = "registry", value_name = "PATH")]
    registry: Option<String>,

    /// Minimum signers required; defaults to participant count
    #[arg(long = "min-signers", value_name = "N")]
    min_signers: Option<usize>,

    /// Charter statement for the DKG group
    #[arg(long = "charter", value_name = "STRING", default_value = "")]
    charter: String,

    /// Print the unsealed invite envelope UR instead of the sealed envelope
    #[arg(long = "unsealed")]
    unsealed: bool,

    /// Participants to include, by pet name or ur:xid identifier
    #[arg(required = true, value_name = "PARTICIPANT")]
    participants: Vec<String>,
}

impl CommandArgs {
    pub fn exec(self) -> Result<()> {
        let selection = self.storage.resolve()?;
        if selection.is_some() && self.unsealed {
            bail!("--unsealed cannot be used with Hubert storage options");
        }

        let invite = build_invite(
            self.registry,
            self.min_signers,
            self.charter,
            self.participants,
        )?;

        if let Some(selection) = selection {
            let envelope = invite.to_envelope()?;
            let arid = ARID::new();

            let runtime = Runtime::new()?;
            runtime.block_on(async move {
                let client = StorageClient::from_selection(selection).await?;
                client.put(&arid, &envelope).await?;
                Ok::<(), anyhow::Error>(())
            })?;

            println!("{}", arid.ur_string());
        } else if self.unsealed {
            let envelope = invite.to_unsealed_envelope()?;
            println!("{}", envelope.ur_string());
        } else {
            let envelope = invite.to_envelope()?;
            println!("{}", envelope.ur_string());
        }

        Ok(())
    }
}

fn build_invite(
    registry_arg: Option<String>,
    min_signers_arg: Option<usize>,
    charter: String,
    participants: Vec<String>,
) -> Result<DkgGroupInvite> {
    let registry_path = participants_file_path(registry_arg.clone())?;
    let registry = Registry::load(&registry_path).with_context(|| {
        format!("Failed to load registry at {}", registry_path.display())
    })?;

    let resolved = resolve_participants(&registry, &participants)?;
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
    let min_signers = min_signers_arg.unwrap_or(participant_count);
    if min_signers < 2 {
        bail!("--min-signers must be at least 2");
    }
    if min_signers > participant_count {
        bail!("--min-signers cannot exceed participant count");
    }

    DkgGroupInvite::new(
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
        charter,
        participant_docs,
        response_arids,
    )
}
