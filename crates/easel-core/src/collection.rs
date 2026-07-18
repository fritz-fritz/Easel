// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! User-curated asset collections and favorites.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::AssetId;

/// Stable identity for a named collection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CollectionId(Uuid);

impl CollectionId {
    /// Creates a new collection identity.
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

impl Default for CollectionId {
    fn default() -> Self {
        Self::new()
    }
}

/// A named set of library assets.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    /// Stable identity.
    pub id: CollectionId,
    /// User-visible name.
    pub name: String,
    /// Ordered membership.
    pub asset_ids: Vec<AssetId>,
}

impl Collection {
    /// Creates an empty collection.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: CollectionId::new(),
            name: name.into(),
            asset_ids: Vec::new(),
        }
    }

    /// Validates name invariants.
    pub fn validate(&self) -> Result<(), CollectionError> {
        if self.name.trim().is_empty() {
            return Err(CollectionError::EmptyName);
        }
        Ok(())
    }

    /// Appends an asset when it is not already a member.
    pub fn add_asset(&mut self, asset_id: AssetId) {
        if !self.asset_ids.contains(&asset_id) {
            self.asset_ids.push(asset_id);
        }
    }

    /// Removes an asset when present.
    pub fn remove_asset(&mut self, asset_id: AssetId) {
        self.asset_ids.retain(|id| *id != asset_id);
    }
}

/// Invalid collection.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CollectionError {
    /// Collection names must contain visible characters.
    #[error("collection name cannot be empty")]
    EmptyName,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_name() {
        let collection = Collection::new("   ");
        assert_eq!(collection.validate(), Err(CollectionError::EmptyName));
    }

    #[test]
    fn add_asset_is_idempotent() {
        let mut collection = Collection::new("Favorites");
        let id = AssetId::new();
        collection.add_asset(id);
        collection.add_asset(id);
        assert_eq!(collection.asset_ids, vec![id]);
    }
}
