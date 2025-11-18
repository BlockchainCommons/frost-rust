use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, anyhow, bail};
use bc_components::{PublicKeys, XID, XIDProvider};
use bc_envelope::prelude::*;
use bc_xid::{XIDDocument, XIDVerifySignature};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Visitor},
    ser::{SerializeMap, SerializeStruct},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantRecord {
    xid_document_ur: String,
    xid_document: XIDDocument,
    public_keys: PublicKeys,
    pet_name: Option<String>,
}

impl ParticipantRecord {
    #[allow(dead_code)]
    pub fn from_document(
        document: XIDDocument,
        xid_document_ur: impl Into<String>,
        pet_name: Option<String>,
    ) -> Result<Self> {
        let raw: String = xid_document_ur.into();
        let sanitized = sanitize_xid_ur(&raw)?;
        Self::build_from_parts(document, sanitized, pet_name)
    }

    pub fn from_signed_xid_ur(
        xid_document_ur: impl Into<String>,
        pet_name: Option<String>,
    ) -> Result<Self> {
        let (raw, document) = parse_signed_xid_document(xid_document_ur)?;
        Self::build_from_parts(document, raw, pet_name)
    }

    pub fn pet_name(&self) -> Option<&str> { self.pet_name.as_deref() }
    pub fn public_keys(&self) -> &PublicKeys { &self.public_keys }
    pub fn xid(&self) -> XID { self.xid_document.xid() }
    #[allow(dead_code)]
    pub fn xid_document(&self) -> &XIDDocument { &self.xid_document }
    #[allow(dead_code)]
    pub fn xid_document_ur(&self) -> &str { &self.xid_document_ur }

    fn build_from_parts(
        document: XIDDocument,
        xid_document_ur: String,
        pet_name: Option<String>,
    ) -> Result<Self> {
        let public_keys = document
            .inception_key()
            .ok_or_else(|| anyhow!("XID document missing inception key"))?
            .public_keys()
            .clone();
        let record = Self {
            xid_document_ur,
            xid_document: document,
            public_keys,
            pet_name,
        };
        Ok(record)
    }

    fn recreate_from_serialized(
        xid_document_ur: String,
        pet_name: Option<String>,
    ) -> Result<Self> {
        let (raw, document) = parse_signed_xid_document(xid_document_ur)?;
        Self::build_from_parts(document, raw, pet_name)
    }
}

impl Serialize for ParticipantRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let field_count = if self.pet_name.is_some() { 2 } else { 1 };
        let mut state =
            serializer.serialize_struct("ParticipantRecord", field_count)?;
        state.serialize_field("xid_document", &self.xid_document_ur)?;
        if let Some(name) = &self.pet_name {
            state.serialize_field("pet_name", name)?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for ParticipantRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        enum Field {
            XidDocument,
            PetName,
        }

        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D2>(deserializer: D2) -> Result<Self, D2::Error>
            where
                D2: Deserializer<'de>,
            {
                struct FieldVisitor;

                impl<'de> Visitor<'de> for FieldVisitor {
                    type Value = Field;

                    fn expecting(
                        &self,
                        formatter: &mut std::fmt::Formatter,
                    ) -> std::fmt::Result {
                        formatter.write_str("`xid_document` or `pet_name`")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Field, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            "xid_document" => Ok(Field::XidDocument),
                            "pet_name" => Ok(Field::PetName),
                            _ => Err(de::Error::unknown_field(
                                value,
                                &["xid_document", "pet_name"],
                            )),
                        }
                    }
                }

                deserializer.deserialize_identifier(FieldVisitor)
            }
        }

        struct ParticipantRecordVisitor;

        impl<'de> Visitor<'de> for ParticipantRecordVisitor {
            type Value = ParticipantRecord;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter,
            ) -> std::fmt::Result {
                formatter.write_str("a participant record")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut xid_document_ur: Option<String> = None;
                let mut pet_name: Option<Option<String>> = None;

                while let Some(field) = map.next_key()? {
                    match field {
                        Field::XidDocument => {
                            if xid_document_ur.is_some() {
                                return Err(de::Error::duplicate_field(
                                    "xid_document",
                                ));
                            }
                            xid_document_ur = Some(map.next_value()?);
                        }
                        Field::PetName => {
                            if pet_name.is_some() {
                                return Err(de::Error::duplicate_field(
                                    "pet_name",
                                ));
                            }
                            pet_name = Some(map.next_value()?);
                        }
                    }
                }

                let xid_document_ur = xid_document_ur
                    .ok_or_else(|| de::Error::missing_field("xid_document"))?;

                ParticipantRecord::recreate_from_serialized(
                    xid_document_ur,
                    pet_name.unwrap_or(None),
                )
                .map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_struct(
            "ParticipantRecord",
            &["xid_document", "pet_name"],
            ParticipantRecordVisitor,
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ParticipantsFile {
    #[serde(default, with = "serde_participants_map")]
    participants: BTreeMap<XID, ParticipantRecord>,
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
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create directory {}", parent.display())
            })?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)
            .with_context(|| format!("Failed to write {}", path.display()))
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

fn parse_signed_xid_document(
    xid_document_ur: impl Into<String>,
) -> Result<(String, XIDDocument)> {
    let raw = xid_document_ur.into();
    let sanitized = sanitize_xid_ur(&raw)?;
    let ur = UR::from_ur_string(&sanitized)
        .with_context(|| format!("Failed to parse UR: {sanitized}"))?;
    if ur.ur_type_str() != "xid" {
        bail!("Expected a ur:xid document, found ur:{}", ur.ur_type_str());
    }

    let envelope = Envelope::from_tagged_cbor(ur.cbor())
        .context("Unable to decode XID document envelope")?;
    let document = XIDDocument::from_envelope(
        &envelope,
        None,
        XIDVerifySignature::Inception,
    )
    .context("XID document must be signed by its inception key")?;

    Ok((sanitized, document))
}

fn sanitize_xid_ur(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("XID document is required");
    }
    Ok(trimmed.to_owned())
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
