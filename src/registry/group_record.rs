use bc_components::XID;
use bc_envelope::prelude::UREncodable;
use bc_ur::URDecodable;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct GroupParticipant {
    #[serde(with = "serde_xid")]
    xid: XID,
}

impl GroupParticipant {
    pub fn new(xid: XID) -> Self { Self { xid } }

    pub fn xid(&self) -> &XID { &self.xid }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ContributionPaths {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round1_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round1_package: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round2_secret: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_package: Option<String>,
}

impl ContributionPaths {
    pub fn merge_missing(&mut self, other: &ContributionPaths) {
        if self.round1_secret.is_none() {
            self.round1_secret = other.round1_secret.clone();
        }
        if self.round1_package.is_none() {
            self.round1_package = other.round1_package.clone();
        }
        if self.round2_secret.is_none() {
            self.round2_secret = other.round2_secret.clone();
        }
        if self.key_package.is_none() {
            self.key_package = other.key_package.clone();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.round1_secret.is_none()
            && self.round1_package.is_none()
            && self.round2_secret.is_none()
            && self.key_package.is_none()
    }
}

/// Tracks pending communication with participants (coordinator-side).
///
/// After each phase, the coordinator stores:
/// - `send_to_arid`: Where to POST the next request to this participant
/// - `collect_from_arid`: Where to GET this participant's response
///
/// The flow is:
/// 1. After invite send: collect_from_arid = where participant will post invite
///    response
/// 2. After round1 collect: send_to_arid = where participant wants Round 2
///    request
/// 3. After round2 send: collect_from_arid = where participant will post Round
///    2 response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PendingRequests {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    requests: Vec<PendingRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PendingRequest {
    #[serde(with = "super::group_record::serde_xid")]
    participant: XID,
    /// Where to POST the next request to this participant (their listening
    /// ARID)
    #[serde(
        default,
        with = "serde_option_arid",
        skip_serializing_if = "Option::is_none"
    )]
    send_to_arid: Option<bc_components::ARID>,
    /// Where to GET this participant's response (coordinator's collection
    /// ARID)
    #[serde(with = "serde_arid")]
    collect_from_arid: bc_components::ARID,
}

impl PendingRequests {
    pub fn new() -> Self { Self { requests: Vec::new() } }

    /// Add a pending request where we only know where to collect from.
    /// Used after invite send (we'll collect invite responses from these
    /// ARIDs).
    pub fn add_collect_only(
        &mut self,
        participant: XID,
        collect_from_arid: bc_components::ARID,
    ) {
        self.requests.push(PendingRequest {
            participant,
            send_to_arid: None,
            collect_from_arid,
        });
    }

    /// Add a pending request where we know where to send AND where to collect.
    /// Used after round2 send (we send to their ARID, collect from our ARID).
    pub fn add_send_and_collect(
        &mut self,
        participant: XID,
        send_to_arid: bc_components::ARID,
        collect_from_arid: bc_components::ARID,
    ) {
        self.requests.push(PendingRequest {
            participant,
            send_to_arid: Some(send_to_arid),
            collect_from_arid,
        });
    }

    /// Add a pending request where we only know where to send.
    /// Used after round1 collect (we extracted where to send Round 2).
    pub fn add_send_only(
        &mut self,
        participant: XID,
        send_to_arid: bc_components::ARID,
    ) {
        // Use a dummy collect_from_arid that will be replaced in round2 send
        self.requests.push(PendingRequest {
            participant,
            send_to_arid: Some(send_to_arid),
            collect_from_arid: send_to_arid, // Placeholder, will be replaced
        });
    }

    pub fn is_empty(&self) -> bool { self.requests.is_empty() }

    /// Iterate over (participant, collect_from_arid) pairs.
    /// Used when collecting responses.
    pub fn iter_collect(
        &self,
    ) -> impl Iterator<Item = (&XID, &bc_components::ARID)> {
        self.requests
            .iter()
            .map(|r| (&r.participant, &r.collect_from_arid))
    }

    /// Iterate over (participant, send_to_arid) pairs.
    /// Used when sending requests. Panics if send_to_arid is None.
    pub fn iter_send(
        &self,
    ) -> impl Iterator<Item = (&XID, &bc_components::ARID)> {
        self.requests.iter().map(|r| {
            (
                &r.participant,
                r.send_to_arid
                    .as_ref()
                    .expect("send_to_arid not set for this request"),
            )
        })
    }

    /// Iterate over full (participant, send_to_arid, collect_from_arid) tuples.
    pub fn iter_full(
        &self,
    ) -> impl Iterator<
        Item = (&XID, Option<&bc_components::ARID>, &bc_components::ARID),
    > {
        self.requests.iter().map(|r| {
            (
                &r.participant,
                r.send_to_arid.as_ref(),
                &r.collect_from_arid,
            )
        })
    }

    pub fn len(&self) -> usize { self.requests.len() }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupRecord {
    charter: String,
    min_signers: usize,
    coordinator: GroupParticipant,
    participants: Vec<GroupParticipant>,
    #[serde(default, skip_serializing_if = "ContributionPaths::is_empty")]
    contributions: ContributionPaths,
    /// ARID where this participant is listening for the coordinator's next
    /// message. Set by participant after responding to invite (expecting
    /// Round 2 request).
    #[serde(
        default,
        with = "serde_option_arid",
        skip_serializing_if = "Option::is_none"
    )]
    listening_at_arid: Option<bc_components::ARID>,
    /// Coordinator's tracking of pending participant communications.
    /// Maps each participant to where to send/collect.
    #[serde(default, skip_serializing_if = "PendingRequests::is_empty")]
    pending_requests: PendingRequests,
}

impl GroupRecord {
    pub fn new(
        charter: String,
        min_signers: usize,
        coordinator: GroupParticipant,
        participants: Vec<GroupParticipant>,
    ) -> Self {
        Self {
            charter,
            min_signers,
            coordinator,
            participants,
            contributions: ContributionPaths::default(),
            listening_at_arid: None,
            pending_requests: PendingRequests::default(),
        }
    }

    pub fn coordinator(&self) -> &GroupParticipant { &self.coordinator }

    pub fn participants(&self) -> &[GroupParticipant] { &self.participants }

    pub fn min_signers(&self) -> usize { self.min_signers }

    pub fn charter(&self) -> &str { &self.charter }

    pub fn contributions(&self) -> &ContributionPaths { &self.contributions }

    pub fn set_contributions(&mut self, contributions: ContributionPaths) {
        self.contributions = contributions;
    }

    pub fn merge_contributions(&mut self, other: &ContributionPaths) {
        self.contributions.merge_missing(other);
    }

    /// Get the ARID where this participant is listening for the next message.
    pub fn listening_at_arid(&self) -> Option<bc_components::ARID> {
        self.listening_at_arid
    }

    /// Set the ARID where this participant is listening for the next message.
    pub fn set_listening_at_arid(&mut self, arid: bc_components::ARID) {
        self.listening_at_arid = Some(arid);
    }

    /// Clear the listening ARID (after receiving the expected message).
    pub fn clear_listening_at_arid(&mut self) { self.listening_at_arid = None; }

    pub fn pending_requests(&self) -> &PendingRequests {
        &self.pending_requests
    }

    pub fn set_pending_requests(&mut self, requests: PendingRequests) {
        self.pending_requests = requests;
    }

    pub fn clear_pending_requests(&mut self) {
        self.pending_requests = PendingRequests::default();
    }

    pub fn config_matches(&self, other: &GroupRecord) -> bool {
        self.charter == other.charter
            && self.min_signers == other.min_signers
            && self.coordinator == other.coordinator
            && self.participants == other.participants
    }
}

mod serde_xid {
    use serde::{Deserialize, Deserializer, Serializer};

    use super::*;

    pub fn serialize<S>(xid: &XID, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&xid.ur_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<XID, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        XID::from_ur_string(&raw).map_err(serde::de::Error::custom)
    }
}

mod serde_arid {
    use bc_components::ARID;
    use bc_envelope::prelude::CBOR;
    use bc_ur::prelude::UR;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(arid: &ARID, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use bc_envelope::prelude::UREncodable;
        serializer.serialize_str(&arid.ur_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ARID, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        let ur = UR::from_ur_string(&raw).map_err(serde::de::Error::custom)?;
        if ur.ur_type_str() != "arid" {
            return Err(serde::de::Error::custom(format!(
                "Expected ur:arid, found ur:{}",
                ur.ur_type_str()
            )));
        }
        let cbor = ur.cbor();
        ARID::try_from(cbor.clone()).or_else(|_| {
            let bytes = CBOR::try_into_byte_string(cbor)
                .map_err(serde::de::Error::custom)?;
            ARID::from_data_ref(bytes).map_err(serde::de::Error::custom)
        })
    }
}

mod serde_option_arid {
    use bc_components::ARID;
    use bc_envelope::prelude::CBOR;
    use bc_ur::prelude::UR;
    use serde::{Deserialize, Deserializer, Serializer, de::IntoDeserializer};

    pub fn serialize<S>(
        value: &Option<ARID>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use bc_envelope::prelude::UREncodable;
        match value {
            Some(arid) => serializer.serialize_some(&arid.ur_string()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<Option<ARID>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw: Option<String> = Option::deserialize(deserializer)?;
        raw.map(|value| deserialize_arid(value.into_deserializer()))
            .transpose()
    }

    fn deserialize_arid<'de, D>(deserializer: D) -> Result<ARID, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        let ur = UR::from_ur_string(&raw).map_err(serde::de::Error::custom)?;
        if ur.ur_type_str() != "arid" {
            return Err(serde::de::Error::custom(format!(
                "Expected ur:arid, found ur:{}",
                ur.ur_type_str()
            )));
        }
        let cbor = ur.cbor();
        ARID::try_from(cbor.clone()).or_else(|_| {
            let bytes = CBOR::try_into_byte_string(cbor)
                .map_err(serde::de::Error::custom)?;
            ARID::from_data_ref(bytes).map_err(serde::de::Error::custom)
        })
    }
}
