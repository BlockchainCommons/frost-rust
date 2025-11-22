#![allow(dead_code)]
use anyhow::Result;
use bc_components::{ARID, XID, XIDProvider};
use bc_envelope::prelude::*;
use bc_xid::{XIDDocument, XIDVerifySignature};

#[derive(Debug, Clone, PartialEq)]
pub struct DkGProposedParticipant {
    ur_string: String,     // The UR encoding of the XID document
    envelope: Envelope,    // The envelope containing the XID document
    document: XIDDocument, // The participant's XID document
    response_arid: ARID,   // ARID of the participant's DKG response
}

impl PartialOrd for DkGProposedParticipant {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.xid().cmp(&other.xid()))
    }
}

impl DkGProposedParticipant {
    pub fn new(ur_string: String, response_arid: ARID) -> Result<Self> {
        let envelope = Envelope::from_ur_string(&ur_string)?;
        let document = XIDDocument::from_envelope(
            &envelope,
            None,
            XIDVerifySignature::Inception,
        )?;
        Ok(Self { ur_string, envelope, document, response_arid })
    }

    pub fn xid(&self) -> XID { self.document.xid() }

    pub fn xid_document(&self) -> &XIDDocument { &self.document }

    pub fn xid_doc_ur(&self) -> &String { &self.ur_string }

    pub fn xid_doc_envelope(&self) -> &Envelope { &self.envelope }

    pub fn response_arid(&self) -> ARID { self.response_arid }
}
