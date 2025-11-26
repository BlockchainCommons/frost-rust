use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use bc_components::{ARID, XID};
use bc_envelope::prelude::{URDecodable, UREncodable};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer, ser::SerializeMap,
};

use super::{GroupRecord, OwnerRecord, ParticipantRecord};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Registry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner: Option<OwnerRecord>,
    #[serde(default, with = "serde_participants_map")]
    participants: BTreeMap<XID, ParticipantRecord>,
    #[serde(default)]
    groups: BTreeMap<String, GroupRecord>,
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
        if let Some(name) = owner.pet_name()
            && let Some((_, existing)) = self.participant_by_pet_name(name)
            && existing.pet_name() == Some(name)
        {
            bail!("Pet name '{name}' already used by a participant");
        }

        match &self.owner {
            None => {
                self.owner = Some(owner);
                Ok(OwnerOutcome::Inserted)
            }
            Some(existing) => {
                if existing.xid() == owner.xid()
                    && existing.xid_document_ur() == owner.xid_document_ur()
                    && existing.pet_name() == owner.pet_name()
                {
                    Ok(OwnerOutcome::AlreadyPresent)
                } else if existing.xid() == owner.xid() {
                    if existing.xid_document_ur() != owner.xid_document_ur() {
                        bail!("Owner already exists with different keys");
                    }
                    self.owner = Some(owner);
                    Ok(OwnerOutcome::Inserted)
                } else {
                    bail!("Owner already recorded for {}", existing.xid());
                }
            }
        }
    }

    pub fn owner(&self) -> Option<&OwnerRecord> { self.owner.as_ref() }

    pub fn add_participant(
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

    pub fn participant(&self, xid: &XID) -> Option<&ParticipantRecord> {
        self.participants.get(xid)
    }

    pub fn participant_by_pet_name(
        &self,
        pet_name: &str,
    ) -> Option<(&XID, &ParticipantRecord)> {
        self.participants
            .iter()
            .find(|(_, record)| record.pet_name() == Some(pet_name))
    }

    pub fn group(&self, group_id: &ARID) -> Option<&GroupRecord> {
        self.groups.get(&group_key(group_id))
    }

    pub fn group_mut(&mut self, group_id: &ARID) -> Option<&mut GroupRecord> {
        self.groups.get_mut(&group_key(group_id))
    }

    pub fn record_group(
        &mut self,
        group_id: ARID,
        record: GroupRecord,
    ) -> Result<GroupOutcome> {
        let key = group_key(&group_id);
        match self.groups.get(&key) {
            Some(existing) => {
                if !existing.config_matches(&record) {
                    bail!(
                        "Group {} already exists with a different configuration",
                        group_id.hex()
                    );
                }
                let mut merged = existing.clone();
                merged.merge_contributions(record.contributions());
                self.groups.insert(key, merged);
                Ok(GroupOutcome::Updated)
            }
            None => {
                self.groups.insert(key, record);
                Ok(GroupOutcome::Inserted)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupOutcome {
    Inserted,
    Updated,
}

fn group_key(group_id: &ARID) -> String { group_id.ur_string() }

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
