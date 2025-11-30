#![allow(dead_code)]
use anyhow::{Context, Result, bail};
use bc_components::{ARID, XID, XIDProvider};
use bc_envelope::prelude::*;
use bc_ur::prelude::UR;
use bc_xid::{XIDDocument, XIDVerifySignature};

#[derive(Debug, Clone, PartialEq)]
pub struct DkgProposedParticipant {
    ur_string: String,     // The UR encoding of the XID document
    envelope: Envelope,    // The envelope containing the XID document
    document: XIDDocument, // The participant's XID document
    response_arid: ARID,   // ARID of the participant's DKG response
}

impl PartialOrd for DkgProposedParticipant {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.xid().cmp(&other.xid()))
    }
}

impl DkgProposedParticipant {
    pub fn new(ur_string: String, response_arid: ARID) -> Result<Self> {
        let (envelope, document) = parse_xid_envelope(&ur_string)?;
        Ok(Self { ur_string, envelope, document, response_arid })
    }

    pub fn xid(&self) -> XID { self.document.xid() }

    pub fn xid_document(&self) -> &XIDDocument { &self.document }

    pub fn xid_document_ur(&self) -> &String { &self.ur_string }

    pub fn xid_document_envelope(&self) -> &Envelope { &self.envelope }

    pub fn response_arid(&self) -> ARID { self.response_arid }
}

fn parse_xid_envelope(input: &str) -> Result<(Envelope, XIDDocument)> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("XID document is required");
    }
    let ur = UR::from_ur_string(trimmed)
        .with_context(|| format!("Failed to parse UR: {trimmed}"))?;
    if ur.ur_type_str() != "xid" && ur.ur_type_str() != "envelope" {
        bail!("Expected a ur:xid document, found ur:{}", ur.ur_type_str());
    }

    let envelope_cbor = ur.cbor();
    let envelope = Envelope::from_tagged_cbor(envelope_cbor.clone())
        .or_else(|_| Envelope::from_untagged_cbor(envelope_cbor))
        .context("Unable to decode XID document envelope")?;
    let document = XIDDocument::from_envelope(
        &envelope,
        None,
        XIDVerifySignature::Inception,
    )?;
    Ok((envelope, document))
}
