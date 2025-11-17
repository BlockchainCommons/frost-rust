use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use bc_components::XIDProvider;
use bc_envelope::prelude::*;
use bc_xid::XIDDocument;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ParticipantRecord {
    xid: String,
    document: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pet_name: Option<String>,
}

impl ParticipantRecord {
    pub fn from_sources(
        document: &XIDDocument,
        envelope: &Envelope,
        pet_name: Option<String>,
    ) -> Result<Self> {
        let xid_hex = document.xid().to_hex();
        let document_ur = UR::new("xid", envelope.to_cbor())
            .context("Failed to encode XID document as UR")?
            .string();
        Ok(Self { xid: xid_hex, document: document_ur, pet_name })
    }

    pub fn document(&self) -> &str { &self.document }
    pub fn pet_name(&self) -> Option<&str> { self.pet_name.as_deref() }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ParticipantsFile {
    #[serde(default)]
    participants: Vec<ParticipantRecord>,
}

impl ParticipantsFile {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let data = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        if data.trim().is_empty() {
            return Ok(Self::default());
        }

        serde_json::from_str(&data)
            .with_context(|| format!("Invalid JSON in {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)
            .with_context(|| format!("Failed to write {}", path.display()))
    }

    pub fn add(&mut self, record: ParticipantRecord) -> Result<AddOutcome> {
        if let Some((name, existing)) = record.pet_name().and_then(|name| {
            self.participants
                .iter()
                .find(|p| p.pet_name() == Some(name))
                .map(|existing| (name, existing))
        }) {
            if existing.document() != record.document() {
                bail!("Pet name '{name}' already used by another participant");
            }
            return Ok(AddOutcome::AlreadyPresent);
        }

        if let Some(existing) = self
            .participants
            .iter()
            .find(|p| p.document() == record.document())
        {
            if existing.pet_name() == record.pet_name() {
                return Ok(AddOutcome::AlreadyPresent);
            }
            bail!("Participant already exists with a different pet name");
        }

        self.participants.push(record);
        Ok(AddOutcome::Inserted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddOutcome {
    Inserted,
    AlreadyPresent,
}
