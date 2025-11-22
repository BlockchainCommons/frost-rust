use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use bc_components::XID;
use bc_envelope::prelude::{URDecodable, UREncodable};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::ser::SerializeMap;

use super::{OwnerRecord, ParticipantRecord};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Registry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner: Option<OwnerRecord>,
    #[serde(default, with = "serde_participants_map")]
    participants: BTreeMap<XID, ParticipantRecord>,
}

impl Registry {
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
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create directory {}", parent.display())
            })?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)
            .with_context(|| format!("Failed to write {}", path.display()))
    }

    pub fn set_owner(&mut self, owner: OwnerRecord) -> Result<OwnerOutcome> {
        match &self.owner {
            None => {
                self.owner = Some(owner);
                Ok(OwnerOutcome::Inserted)
            }
            Some(existing) => {
                if existing.xid() == owner.xid()
                    && existing.xid_document_ur() == owner.xid_document_ur()
                {
                    Ok(OwnerOutcome::AlreadyPresent)
                } else if existing.xid() == owner.xid() {
                    bail!("Owner already exists with different keys");
                } else {
                    bail!("Owner already recorded for {}", existing.xid());
                }
            }
        }
    }

    pub fn add(
        &mut self,
        xid: XID,
        record: ParticipantRecord,
    ) -> Result<AddOutcome> {
        if let Some((name, existing_xid, existing_record)) =
            record.pet_name().and_then(|name| {
                self.participants
                    .iter()
                    .find(|(_, rec)| rec.pet_name() == Some(name))
                    .map(|(existing_xid, existing_record)| {
                        (name, existing_xid, existing_record)
                    })
            })
        {
            if *existing_xid != xid {
                bail!("Pet name '{name}' already used by another participant");
            }
            if existing_record.public_keys() == record.public_keys() {
                return Ok(AddOutcome::AlreadyPresent);
            }
            bail!("Participant already exists with a different pet name");
        }

        match self.participants.get(&xid) {
            Some(existing) => {
                if existing.public_keys() == record.public_keys()
                    && existing.pet_name() == record.pet_name()
                {
                    Ok(AddOutcome::AlreadyPresent)
                } else {
                    bail!(
                        "Participant already exists with a different pet name"
                    );
                }
            }
            None => {
                self.participants.insert(xid, record);
                Ok(AddOutcome::Inserted)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddOutcome {
    Inserted,
    AlreadyPresent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerOutcome {
    Inserted,
    AlreadyPresent,
}

mod serde_participants_map {
    use super::*;

    pub fn serialize<S>(
        map: &BTreeMap<XID, ParticipantRecord>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(map.len()))?;
        for (xid, record) in map {
            state.serialize_entry(&xid.ur_string(), record)?;
        }
        state.end()
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<BTreeMap<XID, ParticipantRecord>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw: BTreeMap<String, ParticipantRecord> =
            BTreeMap::deserialize(deserializer)?;
        raw.into_iter()
            .map(|(ur, record)| {
                let xid = XID::from_ur_string(&ur)
                    .map_err(serde::de::Error::custom)?;
                Ok((xid, record))
            })
            .collect()
    }
}
