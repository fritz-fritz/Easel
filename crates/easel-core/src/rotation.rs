// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Independent wallpaper rotation queues and runtime automation state.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{AssetId, ProfileId, ScheduleId};

/// Current serialized rotation-queue schema.
pub const ROTATION_QUEUE_SCHEMA_VERSION: u16 = 1;

/// Current serialized automation-state schema.
pub const AUTOMATION_STATE_SCHEMA_VERSION: u16 = 1;

/// Stable rotation-queue identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RotationQueueId(Uuid);

impl RotationQueueId {
    /// Creates a new rotation-queue identity.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the canonical hyphenated UUID string.
    #[must_use]
    pub fn to_hyphenated_string(self) -> String {
        self.0.hyphenated().to_string()
    }

    /// Parses a hyphenated UUID string.
    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(value.trim())?))
    }
}

impl Default for RotationQueueId {
    fn default() -> Self {
        Self::new()
    }
}

/// Ordered still-image candidates for unattended rotation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RotationQueue {
    /// Serialized schema version.
    pub schema_version: u16,
    /// Stable identity.
    pub id: RotationQueueId,
    /// User-visible name.
    pub name: String,
    /// Asset identities in preferred playback order.
    pub assets: Vec<AssetId>,
    /// How many recently applied assets to avoid when choosing the next one.
    pub avoid_repeat: u32,
    /// When true, selection shuffles among eligible assets instead of advancing in order.
    pub shuffle: bool,
}

impl RotationQueue {
    /// Creates an empty named queue with avoid-repeat of one.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: ROTATION_QUEUE_SCHEMA_VERSION,
            id: RotationQueueId::new(),
            name: name.into(),
            assets: Vec::new(),
            avoid_repeat: 1,
            shuffle: false,
        }
    }

    /// Validates schema, name, and membership.
    pub fn validate(&self) -> Result<(), RotationQueueError> {
        if self.schema_version != ROTATION_QUEUE_SCHEMA_VERSION {
            return Err(RotationQueueError::UnsupportedSchema(self.schema_version));
        }
        if self.name.trim().is_empty() {
            return Err(RotationQueueError::EmptyName);
        }
        if self.assets.is_empty() {
            return Err(RotationQueueError::EmptyQueue);
        }
        let mut seen = std::collections::HashSet::with_capacity(self.assets.len());
        for asset in &self.assets {
            if !seen.insert(*asset) {
                return Err(RotationQueueError::DuplicateAsset(*asset));
            }
        }
        Ok(())
    }
}

/// Invalid rotation queue.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RotationQueueError {
    /// Schema version is not supported by this build.
    #[error("unsupported rotation queue schema version: {0}")]
    UnsupportedSchema(u16),
    /// Queue names must contain visible characters.
    #[error("rotation queue name cannot be empty")]
    EmptyName,
    /// A queue must reference at least one asset.
    #[error("rotation queue must include at least one asset")]
    EmptyQueue,
    /// Membership lists cannot repeat an asset.
    #[error("rotation queue contains duplicate asset: {0:?}")]
    DuplicateAsset(AssetId),
}

/// Persisted runtime state for unattended rotation.
///
/// Survives restart so pause, history, and avoid-repeat remain explainable.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AutomationState {
    /// Serialized schema version.
    pub schema_version: u16,
    /// When true, schedules do not advance.
    pub paused: bool,
    /// Active profile driving composition, if any.
    pub active_profile_id: Option<ProfileId>,
    /// Active schedule, if any.
    pub active_schedule_id: Option<ScheduleId>,
    /// Active rotation queue, if any.
    pub active_queue_id: Option<RotationQueueId>,
    /// Last successfully applied asset.
    pub current_asset_id: Option<AssetId>,
    /// Unix timestamp (UTC seconds) of the last successful apply.
    pub last_applied_unix: Option<u64>,
    /// Recently applied assets, newest last, used for avoid-repeat.
    pub recent_asset_ids: Vec<AssetId>,
    /// Human-readable explanation of the last scheduler decision.
    pub last_decision: String,
}

impl AutomationState {
    /// Creates an idle, unpaused automation state.
    #[must_use]
    pub fn idle() -> Self {
        Self {
            schema_version: AUTOMATION_STATE_SCHEMA_VERSION,
            paused: false,
            active_profile_id: None,
            active_schedule_id: None,
            active_queue_id: None,
            current_asset_id: None,
            last_applied_unix: None,
            recent_asset_ids: Vec::new(),
            last_decision: String::new(),
        }
    }

    /// Validates schema version.
    pub fn validate(&self) -> Result<(), AutomationStateError> {
        if self.schema_version != AUTOMATION_STATE_SCHEMA_VERSION {
            return Err(AutomationStateError::UnsupportedSchema(self.schema_version));
        }
        Ok(())
    }

    /// Records a successful apply and updates avoid-repeat history.
    pub fn record_apply(
        &mut self,
        asset_id: AssetId,
        applied_at_unix: u64,
        decision: impl Into<String>,
    ) {
        const MAX_RECENT: usize = 64;
        self.current_asset_id = Some(asset_id);
        self.last_applied_unix = Some(applied_at_unix);
        self.last_decision = decision.into();
        self.recent_asset_ids.push(asset_id);
        if self.recent_asset_ids.len() > MAX_RECENT {
            let excess = self.recent_asset_ids.len() - MAX_RECENT;
            self.recent_asset_ids.drain(0..excess);
        }
    }
}

impl Default for AutomationState {
    fn default() -> Self {
        Self::idle()
    }
}

/// Invalid automation state.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum AutomationStateError {
    /// Schema version is not supported by this build.
    #[error("unsupported automation state schema version: {0}")]
    UnsupportedSchema(u16),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_queue() {
        let queue = RotationQueue::new("Desk");
        assert_eq!(queue.validate(), Err(RotationQueueError::EmptyQueue));
    }

    #[test]
    fn record_apply_trims_history() {
        let mut state = AutomationState::idle();
        for _ in 0..70 {
            state.record_apply(AssetId::new(), 1, "test");
        }
        assert_eq!(state.recent_asset_ids.len(), 64);
    }
}
