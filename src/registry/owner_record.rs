use anyhow::{Context, Result, bail};
use bc_components::{XID, XIDProvider};
use bc_envelope::prelude::*;
use bc_xid::{XIDDocument, XIDVerifySignature};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, MapAccess, Visitor},
    ser::SerializeStruct,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerRecord {
    xid_document_ur: String,
    xid_document: XIDDocument,
}

impl OwnerRecord {
    pub fn from_signed_xid_ur(
        xid_document_ur: impl Into<String>,
    ) -> Result<Self> {
        let (raw, document) = parse_relaxed_xid_document(xid_document_ur)?;
        if document.inception_private_keys().is_none() {
            bail!("Owner XID document must include private keys");
        }
        Ok(Self { xid_document_ur: raw, xid_document: document })
    }

    pub fn xid(&self) -> XID { self.xid_document.xid() }

    pub fn xid_document(&self) -> &XIDDocument { &self.xid_document }

    pub fn xid_document_ur(&self) -> &str { &self.xid_document_ur }
}

impl Serialize for OwnerRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("OwnerRecord", 1)?;
        state.serialize_field("xid_document", &self.xid_document_ur)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for OwnerRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        enum Field {
            XidDocument,
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
                        formatter.write_str("`xid_document`")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Field, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            "xid_document" => Ok(Field::XidDocument),
                            _ => Err(de::Error::unknown_field(
                                value,
                                &["xid_document"],
                            )),
                        }
                    }
                }

                deserializer.deserialize_identifier(FieldVisitor)
            }
        }

        struct OwnerRecordVisitor;

        impl<'de> Visitor<'de> for OwnerRecordVisitor {
            type Value = OwnerRecord;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter,
            ) -> std::fmt::Result {
                formatter.write_str("an owner record")
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut xid_document_ur: Option<String> = None;

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
                    }
                }

                let xid_document_ur = xid_document_ur
                    .ok_or_else(|| de::Error::missing_field("xid_document"))?;

                OwnerRecord::from_signed_xid_ur(xid_document_ur)
                    .map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_struct(
            "OwnerRecord",
            &["xid_document"],
            OwnerRecordVisitor,
        )
    }
}

fn parse_relaxed_xid_document(
    xid_document_ur: impl Into<String>,
) -> Result<(String, XIDDocument)> {
    let raw = xid_document_ur.into();
    let sanitized = sanitize_xid_ur(&raw)?;
    let ur = UR::from_ur_string(&sanitized)
        .with_context(|| format!("Failed to parse UR: {sanitized}"))?;
    if ur.ur_type_str() != "xid" && ur.ur_type_str() != "envelope" {
        bail!("Expected a ur:xid document, found ur:{}", ur.ur_type_str());
    }

    let envelope_cbor = ur.cbor();
    let envelope = Envelope::from_tagged_cbor(envelope_cbor.clone())
        .or_else(|_| Envelope::from_untagged_cbor(envelope_cbor))
        .context("Unable to decode XID document envelope")?;
    let document =
        XIDDocument::from_envelope(&envelope, None, XIDVerifySignature::None)
            .context("XID document could not be parsed")?;

    Ok((sanitized, document))
}

fn sanitize_xid_ur(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("XID document is required");
    }
    Ok(trimmed.to_owned())
}
