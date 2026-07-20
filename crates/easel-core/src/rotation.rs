// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Independent rotation queues, avoid-repeat selection, pause, and skip.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{AssetId, CollectionId};

/// Current serialized rotation-queue schema.
pub const ROTATION_QUEUE_SCHEMA_VERSION: u16 = 1;

/// Stable identity for a rotation queue.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RotationQueueId(Uuid);

impl RotationQueueId {
    /// Creates a new rotation queue identity.
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

/// Avoid-repeat and pause policy for a queue.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RotationPolicy {
    /// Do not reselect an asset that appears in the last N history entries for this queue.
    pub avoid_repeat_count: u32,
    /// When true the scheduler holds the current wallpaper and skips auto-advance.
    pub paused: bool,
}

impl Default for RotationPolicy {
    fn default() -> Self {
        Self {
            avoid_repeat_count: 5,
            paused: false,
        }
    }
}

/// Where a rotation queue draws its ordered membership.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RotationSource {
    /// Explicit ordered asset list owned by the queue.
    Assets {
        /// Membership in rotation order.
        asset_ids: Vec<AssetId>,
    },
    /// Library collection membership (resolved at selection time).
    Collection {
        /// Collection providing the ordered assets.
        collection_id: CollectionId,
    },
}

/// Independent wallpaper rotation queue.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RotationQueue {
    /// Serialized schema version.
    pub schema_version: u16,
    /// Stable identity.
    pub id: RotationQueueId,
    /// User-visible name.
    pub name: String,
    /// Membership source.
    pub source: RotationSource,
    /// Avoid-repeat and pause controls.
    pub policy: RotationPolicy,
    /// Index of the next candidate within the resolved membership.
    pub cursor: u32,
}

impl RotationQueue {
    /// Creates a queue from an explicit asset list.
    #[must_use]
    pub fn from_assets(name: impl Into<String>, asset_ids: Vec<AssetId>) -> Self {
        Self {
            schema_version: ROTATION_QUEUE_SCHEMA_VERSION,
            id: RotationQueueId::new(),
            name: name.into(),
            source: RotationSource::Assets { asset_ids },
            policy: RotationPolicy::default(),
            cursor: 0,
        }
    }

    /// Creates a queue bound to a library collection.
    #[must_use]
    pub fn from_collection(name: impl Into<String>, collection_id: CollectionId) -> Self {
        Self {
            schema_version: ROTATION_QUEUE_SCHEMA_VERSION,
            id: RotationQueueId::new(),
            name: name.into(),
            source: RotationSource::Collection { collection_id },
            policy: RotationPolicy::default(),
            cursor: 0,
        }
    }

    /// Validates schema and name invariants.
    pub fn validate(&self) -> Result<(), RotationError> {
        if self.schema_version != ROTATION_QUEUE_SCHEMA_VERSION {
            return Err(RotationError::UnsupportedSchema(self.schema_version));
        }
        if self.name.trim().is_empty() {
            return Err(RotationError::EmptyName);
        }
        if let RotationSource::Assets { asset_ids } = &self.source
            && asset_ids.is_empty()
        {
            return Err(RotationError::EmptyQueue);
        }
        Ok(())
    }
}

/// Explainable selection outcome from a rotation queue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectionDecision {
    /// Chosen asset.
    pub asset_id: AssetId,
    /// Human-readable reason suitable for status/CLI output.
    pub reason: String,
    /// Cursor value to persist after a successful apply.
    pub next_cursor: u32,
}

/// Selects the next asset from `membership`, honoring avoid-repeat and pause.
///
/// `recent` is the newest-first list of recently applied asset ids for this queue.
pub fn select_next(
    queue: &RotationQueue,
    membership: &[AssetId],
    recent: &[AssetId],
) -> Result<SelectionDecision, RotationError> {
    if queue.policy.paused {
        return Err(RotationError::Paused);
    }
    if membership.is_empty() {
        return Err(RotationError::EmptyQueue);
    }

    let len = membership.len();
    let start = (queue.cursor as usize) % len;
    let avoid = queue.policy.avoid_repeat_count as usize;
    let blocked: Vec<AssetId> = recent.iter().take(avoid).copied().collect();

    for offset in 0..len {
        let index = (start + offset) % len;
        let candidate = membership[index];
        if blocked.contains(&candidate) && offset + 1 < len {
            continue;
        }
        let next_cursor = u32::try_from((index + 1) % len).unwrap_or(0);
        let reason = if blocked.contains(&candidate) {
            format!(
                "selected {candidate} at cursor {index} (avoid-repeat exhausted; all remaining recently used)",
                candidate = candidate.to_hyphenated_string()
            )
        } else {
            format!(
                "selected {candidate} at cursor {index} (avoided last {avoid} applies)",
                candidate = candidate.to_hyphenated_string()
            )
        };
        return Ok(SelectionDecision {
            asset_id: candidate,
            reason,
            next_cursor,
        });
    }

    Err(RotationError::EmptyQueue)
}

/// Advances the cursor as if the current candidate were skipped without applying.
pub fn skip_current(
    queue: &RotationQueue,
    membership: &[AssetId],
) -> Result<(u32, AssetId), RotationError> {
    if membership.is_empty() {
        return Err(RotationError::EmptyQueue);
    }
    let len = membership.len();
    let index = (queue.cursor as usize) % len;
    let skipped = membership[index];
    let next_cursor = u32::try_from((index + 1) % len).unwrap_or(0);
    Ok((next_cursor, skipped))
}

/// Invalid rotation queue or selection state.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RotationError {
    /// No migration exists for the serialized schema.
    #[error("unsupported rotation queue schema version: {0}")]
    UnsupportedSchema(u16),
    /// Queue names must contain visible characters.
    #[error("rotation queue name cannot be empty")]
    EmptyName,
    /// A queue needs at least one asset to rotate.
    #[error("rotation queue has no assets")]
    EmptyQueue,
    /// Auto-advance is paused.
    #[error("rotation queue is paused")]
    Paused,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avoid_repeat_skips_recent() {
        let a = AssetId::new();
        let b = AssetId::new();
        let c = AssetId::new();
        let mut queue = RotationQueue::from_assets("Desk", vec![a, b, c]);
        queue.policy.avoid_repeat_count = 2;
        queue.cursor = 0;
        let decision = select_next(&queue, &[a, b, c], &[a, b]).expect("select");
        assert_eq!(decision.asset_id, c);
        assert_eq!(decision.next_cursor, 0);
    }

    #[test]
    fn paused_queue_refuses_selection() {
        let mut queue = RotationQueue::from_assets("Desk", vec![AssetId::new()]);
        queue.policy.paused = true;
        assert_eq!(
            select_next(&queue, &[AssetId::new()], &[]),
            Err(RotationError::Paused)
        );
    }

    #[test]
    fn skip_advances_cursor() {
        let a = AssetId::new();
        let b = AssetId::new();
        let queue = RotationQueue::from_assets("Desk", vec![a, b]);
        let (next, skipped) = skip_current(&queue, &[a, b]).expect("skip");
        assert_eq!(skipped, a);
        assert_eq!(next, 1);
    }
}
