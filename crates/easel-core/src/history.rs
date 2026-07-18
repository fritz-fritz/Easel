// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Immutable library history events.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AssetId;

/// Stable identity for a history event.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HistoryEventId(Uuid);

impl HistoryEventId {
    /// Creates a new history event identity.
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

impl Default for HistoryEventId {
    fn default() -> Self {
        Self::new()
    }
}

/// What the user did with an asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryAction {
    /// Asset was discovered through search or indexing.
    Discovered,
    /// Asset was opened for composition preview.
    Previewed,
    /// Asset was applied as wallpaper.
    Applied,
    /// Asset was added to favorites.
    Favorited,
    /// Asset was added to a named collection.
    Collected,
}

/// One append-only history record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistoryEvent {
    /// Stable identity.
    pub id: HistoryEventId,
    /// Asset touched by the event.
    pub asset_id: AssetId,
    /// Action performed.
    pub action: HistoryAction,
    /// Unix timestamp in seconds (UTC).
    pub occurred_at_unix: u64,
}

impl HistoryEvent {
    /// Creates a history event for the given asset and action.
    #[must_use]
    pub fn new(asset_id: AssetId, action: HistoryAction, occurred_at_unix: u64) -> Self {
        Self {
            id: HistoryEventId::new(),
            asset_id,
            action,
            occurred_at_unix,
        }
    }
}
