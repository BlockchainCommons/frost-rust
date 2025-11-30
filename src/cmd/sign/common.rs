//! Signing-specific utilities.
//!
//! This module contains utilities specific to threshold signing operations,
//! such as signing state directory management.
//!
//! For cross-cutting utilities shared with DKG, see [`crate::cmd::common`].

use std::path::{Path, PathBuf};

use bc_components::ARID;

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
