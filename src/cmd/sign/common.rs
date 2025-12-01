//! Signing-specific utilities.
//!
//! This module contains utilities specific to threshold signing operations,
//! such as signing state directory management.
//!
//! For cross-cutting utilities shared with DKG, see [`crate::cmd::common`].

use std::path::{Path, PathBuf};

use bc_components::ARID;
use bc_envelope::prelude::*;

use super::super::common::group_state_dir;

/// Returns the signing state directory for a group (without session).
///
/// Path: `{registry_dir}/group-state/{group_id.hex()}/signing`
pub fn signing_state_dir_for_group(
    registry_path: &Path,
    group_id: &ARID,
) -> PathBuf {
    group_state_dir(registry_path, group_id).join("signing")
}

/// Returns the signing state directory for a specific session.
///
/// Path: `{registry_dir}/group-state/{group_id.hex()}/signing/{session_id.
/// hex()}`
pub fn signing_state_dir(
    registry_path: &Path,
    group_id: &ARID,
    session_id: &ARID,
) -> PathBuf {
    signing_state_dir_for_group(registry_path, group_id).join(session_id.hex())
}

/// Content wrapper for signFinalize events.
///
/// This wraps an envelope with a unit subject and type assertion "signFinalize",
/// implementing the traits required by `SealedEvent<T>`.
#[derive(Debug, Clone, PartialEq)]
pub struct SignFinalizeContent {
    envelope: Envelope,
}

impl SignFinalizeContent {
    /// Creates a new SignFinalizeContent with a unit subject and type assertion.
    pub fn new() -> Self {
        Self {
            envelope: Envelope::unit().add_type("signFinalize"),
        }
    }

    /// Adds an assertion to the content envelope.
    pub fn add_assertion(
        mut self,
        predicate: impl EnvelopeEncodable,
        object: impl EnvelopeEncodable,
    ) -> Self {
        self.envelope = self.envelope.add_assertion(predicate, object);
        self
    }

    /// Returns the inner envelope.
    pub fn envelope(&self) -> &Envelope {
        &self.envelope
    }
}

impl Default for SignFinalizeContent {
    fn default() -> Self {
        Self::new()
    }
}

impl From<SignFinalizeContent> for Envelope {
    fn from(content: SignFinalizeContent) -> Self {
        content.envelope
    }
}

impl TryFrom<Envelope> for SignFinalizeContent {
    type Error = bc_envelope::Error;

    fn try_from(envelope: Envelope) -> bc_envelope::Result<Self> {
        // Validate it has a unit subject and type "signFinalize"
        envelope.check_subject_unit()?;
        envelope.check_type("signFinalize")?;
        Ok(Self { envelope })
    }
}
