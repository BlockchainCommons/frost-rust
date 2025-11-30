#![allow(dead_code)]
use anyhow::Result;
use bc_components::{ARID, XIDProvider};
use bc_envelope::prelude::*;
use bc_xid::{XIDDocument, XIDVerifySignature};
use gstp::{
    SealedRequest, SealedRequestBehavior, SealedResponse,
    SealedResponseBehavior,
};

use super::DkgProposedParticipant;

#[derive(Debug, Clone, PartialEq)]
pub struct DkgInvite {
    // ARID of the invite request
    request_id: ARID,
    // XID document of the sender
    sender: XIDDocument,
    // Identifies the DKG group
    group_id: ARID,
    // Date the invite was sent
    date: Date,
    // Expiration date of the invite
    valid_until: Date,
    // Minimum signers required for the DKG group
    min_signers: usize,
    // Charter statement for the DKG group (may be empty)
    charter: String,
    // Identifies participants and their indexes
    ordered_participants: Vec<DkgProposedParticipant>,
}

impl DkgInvite {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: ARID,
        sender: XIDDocument,
        group_id: ARID,
        date: Date,
        valid_until: Date,
        min_signers: usize,
        charter: String,
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
        if min_signers < 2 {
            anyhow::bail!("min_signers must be at least 2");
        }
        let mut ordered_participants = participants
            .into_iter()
            .zip(response_arids.into_iter())
            .map(|(ur_string, response_arid)| {
                DkgProposedParticipant::new(ur_string, response_arid)
            })
            .collect::<Result<Vec<DkgProposedParticipant>>>()?;
        if min_signers > ordered_participants.len() {
            anyhow::bail!("min_signers cannot exceed number of participants");
        }
        ordered_participants.sort_by_key(|p| p.xid());
        Ok(Self {
            request_id,
            sender,
            group_id,
            date,
            valid_until,
            min_signers,
            charter,
            ordered_participants,
        })
    }

    pub fn request_id(&self) -> ARID { self.request_id }

    pub fn sender(&self) -> XIDDocument { self.sender.clone() }

    pub fn group_id(&self) -> ARID { self.group_id }

    pub fn date(&self) -> Date { self.date }

    pub fn valid_until(&self) -> Date { self.valid_until }

    pub fn min_signers(&self) -> usize { self.min_signers }

    pub fn charter(&self) -> &str { &self.charter }

    pub fn participants(&self) -> &Vec<DkgProposedParticipant> {
        &self.ordered_participants
    }

    pub fn to_request(&self) -> Result<SealedRequest> {
        let mut request =
            SealedRequest::new("dkgInvite", self.request_id(), self.sender())
                .with_parameter("group", self.group_id())
                .with_parameter("minSigners", self.min_signers as u64)
                .with_parameter("charter", self.charter.clone())
                .with_date(self.date())
                .with_parameter("validUntil", self.valid_until());
        for participant in self.participants() {
            let xid_document_envelope = participant.xid_document_envelope();
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
            let participant = xid_document_envelope
                .wrap()
                .add_assertion("response_arid", encrypted_response_arid);
            request = request.with_parameter("participant", participant);
        }
        Ok(request)
    }

    /// Creates a signed but unencrypted envelope for auditing/preview.
    pub fn to_unsealed_envelope(&self) -> Result<Envelope> {
        let request = self.to_request()?;
        let sender = self.sender();
        let signer_private_keys =
            sender.inception_private_keys().ok_or_else(|| {
                anyhow::anyhow!(
                    "Sender XID document has no inception signing key"
                )
            })?;
        let envelope = request.to_envelope(
            Some(self.valid_until()),
            Some(signer_private_keys),
            None, // No recipient = signed but not encrypted
        )?;
        Ok(envelope)
    }

    /// Creates a sealed envelope encrypted to all participants.
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
pub enum DkgInvitationResult {
    Accepted,
    Declined(String), // Reason for declining
}

#[derive(Debug, Clone, PartialEq)]
pub struct DkgInvitation {
    response_arid: ARID, // Hubert ARID at which to post the response
    valid_until: Date,   // Expiration date of the invite
    sender: XIDDocument, // Coordinator who sent the invite
    request_id: ARID,    // The GSTP request ID for correlated responses
    peer_continuation: Option<Envelope>, /* Continuation (if any) to return
                          * to sender */
    min_signers: usize, // Minimum signers required
    charter: String,    // Charter text (may be empty)
    group_id: ARID,     // Identifier for the DKG group
}

impl DkgInvitation {
    pub fn response_arid(&self) -> ARID { self.response_arid }

    pub fn valid_until(&self) -> Date { self.valid_until }

    pub fn sender(&self) -> XIDDocument { self.sender.clone() }

    pub fn request_id(&self) -> ARID { self.request_id }

    pub fn peer_continuation(&self) -> Option<&Envelope> {
        self.peer_continuation.as_ref()
    }

    pub fn min_signers(&self) -> usize { self.min_signers }

    pub fn charter(&self) -> &str { &self.charter }

    pub fn group_id(&self) -> ARID { self.group_id }

    /// Build a GSTP response for this invitation result.
    pub fn to_response(
        &self,
        response: DkgInvitationResult,
        recipient: &XIDDocument,
    ) -> SealedResponse {
        let base = match response {
            DkgInvitationResult::Accepted => {
                SealedResponse::new_success(self.request_id, recipient.clone())
            }
            DkgInvitationResult::Declined(reason) => {
                SealedResponse::new_failure(self.request_id, recipient.clone())
                    .with_error(reason)
            }
        };

        base.with_peer_continuation(self.peer_continuation())
    }

    /// Create a signed/encrypted GSTP envelope containing the response for the
    /// coordinator.
    pub fn to_envelope(
        &self,
        response: DkgInvitationResult,
        recipient: &XIDDocument,
    ) -> Result<Envelope> {
        let response = self.to_response(response, recipient);
        let signer_private_keys =
            recipient.inception_private_keys().ok_or_else(|| {
                anyhow::anyhow!("Recipient XID document has no signing keys")
            })?;
        let recipients = [self.sender()];
        Ok(response.to_envelope_for_recipients(
            Some(self.valid_until()),
            Some(signer_private_keys),
            &recipients.iter().collect::<Vec<_>>(),
        )?)
    }

    /// Reverses `DkgInvite::to_envelope` for a single participant.
    ///
    /// - Verifies the envelope is properly encrypted to the recipient.
    /// - Verifies the decrypted envelope is a valid DKG invite from the
    ///   expected sender.
    /// - Verifies the participant is included in the invite.
    /// - Decrypts the participant's response ARID.
    /// - Extracts the `valid_until` date and ensures that it has not expired (>
    ///   now).
    pub fn from_invite(
        invite: Envelope,
        now: Date,
        expected_sender: Option<&XIDDocument>,
        recipient: &XIDDocument,
    ) -> Result<Self> {
        let recipient_private_keys =
            recipient.inception_private_keys().ok_or_else(|| {
                anyhow::anyhow!(
                    "Recipient XID document has no inception private keys"
                )
            })?;

        let sealed_request = SealedRequest::try_from_envelope(
            &invite,
            None,
            Some(now),
            recipient_private_keys,
        )?;

        if let Some(expected_sender) = expected_sender
            && sealed_request.sender().xid() != expected_sender.xid()
        {
            anyhow::bail!("Invite sender does not match expected sender");
        }

        if sealed_request.request().function() != &Function::from("dkgInvite") {
            anyhow::bail!("Unexpected invite function");
        }

        let valid_until: Date = sealed_request
            .request()
            .extract_object_for_parameter("validUntil")?;
        if valid_until <= now {
            anyhow::bail!("Invitation expired");
        }

        let recipient_xid = recipient.xid();
        let min_signers: usize = sealed_request
            .request()
            .extract_object_for_parameter("minSigners")?;
        let charter: String = sealed_request
            .request()
            .extract_object_for_parameter("charter")?;
        let group_id: ARID = sealed_request
            .request()
            .extract_object_for_parameter("group")?;
        let participant_objects = sealed_request
            .request()
            .objects_for_parameter("participant");
        if min_signers < 2 {
            anyhow::bail!("min_signers must be at least 2");
        }
        if min_signers > participant_objects.len() {
            anyhow::bail!("min_signers exceeds participant count");
        }

        for participant in participant_objects {
            let xid_document_envelope = participant.try_unwrap()?;
            let xid_document = XIDDocument::from_envelope(
                &xid_document_envelope,
                None,
                XIDVerifySignature::Inception,
            )?;

            if xid_document.xid() != recipient_xid {
                continue;
            }

            let encrypted_response_arid =
                participant.object_for_predicate("response_arid")?;
            let response_arid_envelope = encrypted_response_arid
                .decrypt_to_recipient(recipient_private_keys)?;
            let response_arid =
                response_arid_envelope.extract_subject::<ARID>()?;

            return Ok(Self {
                response_arid,
                valid_until,
                sender: sealed_request.sender().clone(),
                request_id: sealed_request.request().id(),
                peer_continuation: sealed_request.peer_continuation().cloned(),
                min_signers,
                charter,
                group_id,
            });
        }

        anyhow::bail!("Recipient not found in invite");
    }
}
