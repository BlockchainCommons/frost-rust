use bc_components::{ARID, XID};
use bc_envelope::prelude::{CBOR, URDecodable, UREncodable};
use bc_ur::prelude::UR;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GroupParticipant {
    #[serde(with = "serde_xid")]
    xid: XID,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pet_name: Option<String>,
}

impl GroupParticipant {
    pub fn new(xid: XID, pet_name: Option<String>) -> Self {
        Self { xid, pet_name }
    }

    pub fn xid(&self) -> &XID { &self.xid }

    pub fn pet_name(&self) -> Option<&str> { self.pet_name.as_deref() }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum GroupStatus {
    #[default]
    Pending,
    Accepted,
    Rejected {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
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
    #[serde(with = "serde_arid")]
    request_id: ARID,
    #[serde(with = "serde_arid")]
    response_arid: ARID,
    #[serde(default)]
    status: GroupStatus,
    #[serde(default, skip_serializing_if = "ContributionPaths::is_empty")]
    contributions: ContributionPaths,
    #[serde(
        default,
        with = "serde_option_arid",
        skip_serializing_if = "Option::is_none"
    )]
    next_response_arid: Option<ARID>,
}

impl GroupRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        charter: String,
        min_signers: usize,
        coordinator: GroupParticipant,
        participants: Vec<GroupParticipant>,
        request_id: ARID,
        response_arid: ARID,
        status: GroupStatus,
    ) -> Self {
        Self {
            charter,
            min_signers,
            coordinator,
            participants,
            request_id,
            response_arid,
            status,
            contributions: ContributionPaths::default(),
            next_response_arid: None,
        }
    }

    pub fn coordinator(&self) -> &GroupParticipant { &self.coordinator }

    pub fn participants(&self) -> &[GroupParticipant] { &self.participants }

    pub fn min_signers(&self) -> usize { self.min_signers }

    pub fn charter(&self) -> &str { &self.charter }

    pub fn response_arid(&self) -> ARID { self.response_arid }

    pub fn request_id(&self) -> ARID { self.request_id }

    pub fn status(&self) -> &GroupStatus { &self.status }

    pub fn status_mut(&mut self) -> &mut GroupStatus { &mut self.status }

    pub fn contributions_mut(&mut self) -> &mut ContributionPaths {
        &mut self.contributions
    }

    pub fn contributions(&self) -> &ContributionPaths { &self.contributions }

    pub fn set_next_response_arid(&mut self, arid: ARID) {
        self.next_response_arid = Some(arid);
    }

    pub fn next_response_arid(&self) -> Option<ARID> { self.next_response_arid }

    pub fn merge_contributions(&mut self, other: &ContributionPaths) {
        self.contributions.merge_missing(other);
    }

    pub fn set_contributions(&mut self, contributions: ContributionPaths) {
        self.contributions = contributions;
    }

    pub fn set_status(&mut self, status: GroupStatus) { self.status = status; }

    pub fn update_response_arid(&mut self, response_arid: ARID) {
        self.response_arid = response_arid;
    }

    pub fn update_request_id(&mut self, request_id: ARID) {
        self.request_id = request_id;
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
    use serde::{Deserialize, Deserializer, Serializer};

    use super::*;

    pub fn serialize<S>(arid: &ARID, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
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
    use serde::{Deserialize, Deserializer, Serializer, de::IntoDeserializer};

    use super::*;

    pub fn serialize<S>(
        value: &Option<ARID>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
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
        raw.map(|value| serde_arid::deserialize(value.into_deserializer()))
            .transpose()
    }
}
