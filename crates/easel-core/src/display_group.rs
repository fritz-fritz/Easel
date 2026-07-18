// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Named subsets of displays that share one composition.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::DisplayId;

/// Stable identity for a display group.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DisplayGroupId(Uuid);

impl DisplayGroupId {
    /// Creates a new display group identity.
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

impl Default for DisplayGroupId {
    fn default() -> Self {
        Self::new()
    }
}

/// A reusable named set of displays for spanning compositions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayGroup {
    /// Stable identity.
    pub id: DisplayGroupId,
    /// User-visible name.
    pub name: String,
    /// Participating displays in composition order.
    pub displays: Vec<DisplayId>,
}

impl DisplayGroup {
    /// Creates a group with the given name and members.
    #[must_use]
    pub fn new(name: impl Into<String>, displays: Vec<DisplayId>) -> Self {
        Self {
            id: DisplayGroupId::new(),
            name: name.into(),
            displays,
        }
    }

    /// Validates name and membership invariants.
    pub fn validate(&self) -> Result<(), DisplayGroupError> {
        if self.name.trim().is_empty() {
            return Err(DisplayGroupError::EmptyName);
        }
        if self.displays.is_empty() {
            return Err(DisplayGroupError::EmptyMembership);
        }
        let mut seen = HashSet::with_capacity(self.displays.len());
        for id in &self.displays {
            if !seen.insert(*id) {
                return Err(DisplayGroupError::DuplicateDisplay(*id));
            }
        }
        Ok(())
    }
}

/// Invalid display group.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DisplayGroupError {
    /// Group names must contain visible characters.
    #[error("display group name cannot be empty")]
    EmptyName,
    /// A group must reference at least one display.
    #[error("display group must include at least one display")]
    EmptyMembership,
    /// Membership lists cannot repeat a display.
    #[error("display group contains duplicate display: {0:?}")]
    DuplicateDisplay(DisplayId),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_membership() {
        let group = DisplayGroup::new("Desk", Vec::new());
        assert_eq!(group.validate(), Err(DisplayGroupError::EmptyMembership));
    }

    #[test]
    fn rejects_duplicate_members() {
        let id = DisplayId::from_u128(1);
        let group = DisplayGroup::new("Desk", vec![id, id]);
        assert_eq!(
            group.validate(),
            Err(DisplayGroupError::DuplicateDisplay(id))
        );
    }
}
