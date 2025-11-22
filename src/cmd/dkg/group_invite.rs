#![allow(dead_code)]
use anyhow::Result;
use bc_components::{ARID, XID, XIDProvider};
use bc_envelope::prelude::*;
use bc_xid::{XIDDocument, XIDVerifySignature};
use gstp::SealedRequest;

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

#[derive(Debug, Clone, PartialEq)]
pub struct DkgGroupInvite {
    // ARID of the invite request
    request_id: ARID,
    // XID document of the sender
    sender: XIDDocument,
    // Identifies the DKG session
    session_id: ARID,
    // Date the invite was sent
    date: Date,
    // Expiration date of the invite
    valid_until: Date,
    // Identifies participants and their indexes
    ordered_participants: Vec<DkGProposedParticipant>,
}

impl DkgGroupInvite {
    pub fn new(
        request_id: ARID,
        sender: XIDDocument,
        session_id: ARID,
        date: Date,
        valid_until: Date,
        participants: Vec<String>,
        response_arids: Vec<ARID>,
    ) -> Result<Self> {
        if participants.len() != response_arids.len() {
            anyhow::bail!(
                "Number of participants ({}) does not match number of response ARIDs ({})",
                participants.len(),
                response_arids.len()
            );
        }
        let mut ordered_participants = participants
            .into_iter()
            .zip(response_arids.into_iter())
            .map(|(ur_string, response_arid)| {
                DkGProposedParticipant::new(ur_string, response_arid)
            })
            .collect::<Result<Vec<DkGProposedParticipant>>>()?;
        ordered_participants.sort_by_key(|p| p.xid());
        Ok(Self {
            request_id,
            sender,
            session_id,
            date,
            valid_until,
            ordered_participants,
        })
    }

    pub fn request_id(&self) -> ARID { self.request_id }

    pub fn sender(&self) -> XIDDocument { self.sender.clone() }

    pub fn session_id(&self) -> ARID { self.session_id }

    pub fn date(&self) -> Date { self.date }

    pub fn valid_until(&self) -> Date { self.valid_until }

    pub fn participants(&self) -> &Vec<DkGProposedParticipant> {
        &self.ordered_participants
    }

    pub fn to_request(&self) -> Result<SealedRequest> {
        let mut request = SealedRequest::new(
            "dkgGroupInvite",
            self.request_id(),
            self.sender(),
        )
        .with_parameter("session", self.session_id())
        .with_date(self.date())
        .with_parameter("validUntil", self.valid_until());
        for participant in self.participants() {
            let xid_doc_envelope = participant.xid_doc_envelope();
            let response_arid = participant.response_arid();
            let encryption_key = participant
                .xid_document()
                .encryption_key()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Participant XID document has no encryption key"
                    )
                })?;
            let encrypted_response_arid = response_arid
                .to_envelope()
                .encrypt_to_recipient(encryption_key);
            let participant = xid_doc_envelope
                .wrap()
                .add_assertion("response_arid", encrypted_response_arid);
            request = request.with_parameter("participant", participant);
        }
        Ok(request)
    }

    pub fn to_envelope(&self) -> Result<Envelope> {
        let request = self.to_request()?;
        let sender = self.sender();
        let signer_private_keys =
            sender.inception_private_keys().ok_or_else(|| {
                anyhow::anyhow!(
                    "Sender XID document has no inception signing key"
                )
            })?;
        let recipients: Vec<&XIDDocument> = self
            .participants()
            .iter()
            .map(|p| p.xid_document())
            .collect();
        let envelope = request.to_envelope_for_recipients(
            Some(self.valid_until()),
            Some(signer_private_keys),
            &recipients,
        )?;
        Ok(envelope)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DkgInvitation {
    xid: XID,               // XID of the participant
    response_arid: ARID,    // Hubert ARID at which to post the response
    valid_until: Date,      // Expiration date of the invite
}

impl DkgInvitation {
    /// Reverses `DkgGroupInvite::to_envelope` for a single participant.
    ///
    /// - Verifies the envelope is properly encrypted to the recipient.
    /// - Verifies the decrypted envelope is a valid DKG group invite from the expected sender.
    /// - Verifies the participant is included in the invite.
    /// - Decrypts the participant's response ARID.
    /// - Extracts the `valid_until` date and ensures that it has not expired (> now).
    pub fn from_invite(invite: Envelope, now: Date, expected_sender: &XIDDocument, recipient: &XIDDocument) -> Result<Self> {
        todo!();
    }
}
