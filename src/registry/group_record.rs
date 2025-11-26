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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupRecord {
    charter: String,
    min_signers: usize,
    coordinator: GroupParticipant,
    participants: Vec<GroupParticipant>,
    #[serde(default, skip_serializing_if = "ContributionPaths::is_empty")]
    contributions: ContributionPaths,
    /// ARID where we expect the coordinator's next message (e.g., Round 2)
    #[serde(
        default,
        with = "serde_option_arid",
        skip_serializing_if = "Option::is_none"
    )]
    pending_response: Option<bc_components::ARID>,
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
            pending_response: None,
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

    pub fn pending_response(&self) -> Option<bc_components::ARID> {
        self.pending_response
    }

    pub fn set_pending_response(&mut self, arid: bc_components::ARID) {
        self.pending_response = Some(arid);
    }

    pub fn clear_pending_response(&mut self) { self.pending_response = None; }

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
