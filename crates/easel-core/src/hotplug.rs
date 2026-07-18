// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Display hotplug policy and recovery for missing profile outputs.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Display, DisplayId, Profile};

/// Current serialized hotplug-policy schema.
pub const HOTPLUG_POLICY_SCHEMA_VERSION: u16 = 1;

/// How to behave when a profile references displays that are not currently connected.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingOutputPolicy {
    /// Apply only to the intersection of profile displays and currently connected outputs.
    #[default]
    SkipMissing,
    /// Hold the previous wallpaper until every profile display is present again.
    DeferUntilComplete,
    /// Apply using every currently connected display (ignore the profile's display list).
    UseAllConnected,
}

/// Topology-change recovery settings persisted beside profiles.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HotplugPolicy {
    /// Serialized schema version.
    pub schema_version: u16,
    /// Behavior when profile outputs are missing.
    pub on_missing: MissingOutputPolicy,
    /// When true, reconnects re-run arrangement matching before the next apply.
    pub rematch_on_reconnect: bool,
}

impl Default for HotplugPolicy {
    fn default() -> Self {
        Self {
            schema_version: HOTPLUG_POLICY_SCHEMA_VERSION,
            on_missing: MissingOutputPolicy::SkipMissing,
            rematch_on_reconnect: true,
        }
    }
}

impl HotplugPolicy {
    /// Validates schema invariants.
    pub fn validate(&self) -> Result<(), HotplugError> {
        if self.schema_version != HOTPLUG_POLICY_SCHEMA_VERSION {
            return Err(HotplugError::UnsupportedSchema(self.schema_version));
        }
        Ok(())
    }
}

/// Outcome of resolving a profile's display targets against the live topology.
#[derive(Clone, Debug, PartialEq)]
pub struct HotplugResolution {
    /// Displays that should receive the next apply, in profile order when possible.
    pub active_displays: Vec<Display>,
    /// Profile display ids that are not currently connected.
    pub missing: Vec<DisplayId>,
    /// Whether the apply should proceed with `active_displays`.
    pub should_apply: bool,
    /// Explainable reason for diagnostics, tray, and CLI status.
    pub reason: String,
}

/// Resolves which connected displays a profile should use under `policy`.
#[must_use]
pub fn resolve_displays(
    profile: &Profile,
    connected: &[Display],
    policy: &HotplugPolicy,
) -> HotplugResolution {
    let wanted = effective_display_ids(profile);
    if wanted.is_empty() {
        return HotplugResolution {
            active_displays: connected.to_vec(),
            missing: Vec::new(),
            should_apply: !connected.is_empty(),
            reason: if connected.is_empty() {
                "no connected displays".into()
            } else {
                "profile has no display list; using all connected outputs".into()
            },
        };
    }

    let mut active = Vec::new();
    let mut missing = Vec::new();
    for id in &wanted {
        if let Some(display) = connected.iter().find(|display| display.id == *id) {
            active.push(display.clone());
        } else {
            missing.push(*id);
        }
    }

    match policy.on_missing {
        MissingOutputPolicy::SkipMissing => HotplugResolution {
            should_apply: !active.is_empty(),
            reason: if missing.is_empty() {
                format!("all {} profile display(s) connected", active.len())
            } else if active.is_empty() {
                "all profile displays missing; skipping apply".into()
            } else {
                format!(
                    "applying to {} of {} profile display(s); {} missing",
                    active.len(),
                    wanted.len(),
                    missing.len()
                )
            },
            active_displays: active,
            missing,
        },
        MissingOutputPolicy::DeferUntilComplete => {
            let complete = missing.is_empty() && !active.is_empty();
            HotplugResolution {
                should_apply: complete,
                reason: if complete {
                    format!("all {} profile display(s) connected", active.len())
                } else {
                    format!(
                        "deferring apply until {} missing display(s) reconnect",
                        missing.len()
                    )
                },
                active_displays: active,
                missing,
            }
        }
        MissingOutputPolicy::UseAllConnected => HotplugResolution {
            active_displays: connected.to_vec(),
            missing,
            should_apply: !connected.is_empty(),
            reason: if connected.is_empty() {
                "no connected displays".into()
            } else {
                format!(
                    "using all {} connected display(s) ({} profile display(s) missing)",
                    connected.len(),
                    wanted.len().saturating_sub(
                        connected
                            .iter()
                            .filter(|display| wanted.contains(&display.id))
                            .count()
                    )
                )
            },
        },
    }
}

fn effective_display_ids(profile: &Profile) -> Vec<DisplayId> {
    profile.displays.clone()
}

/// Invalid hotplug policy.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum HotplugError {
    /// No migration exists for the serialized schema.
    #[error("unsupported hotplug policy schema version: {0}")]
    UnsupportedSchema(u16),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::two_equal_row;

    #[test]
    fn skip_missing_applies_intersection() {
        let arrangement = two_equal_row();
        let mut profile = Profile::new("Desk");
        let a = arrangement.displays[0].id;
        let b = arrangement.displays[1].id;
        profile.displays = vec![a, b];
        let connected = vec![arrangement.displays[0].clone()];
        let resolution = resolve_displays(&profile, &connected, &HotplugPolicy::default());
        assert!(resolution.should_apply);
        assert_eq!(resolution.active_displays.len(), 1);
        assert_eq!(resolution.missing, vec![b]);
    }

    #[test]
    fn defer_waits_for_complete_set() {
        let arrangement = two_equal_row();
        let mut profile = Profile::new("Desk");
        profile.displays = arrangement
            .displays
            .iter()
            .map(|display| display.id)
            .collect();
        let connected = vec![arrangement.displays[0].clone()];
        let policy = HotplugPolicy {
            on_missing: MissingOutputPolicy::DeferUntilComplete,
            ..HotplugPolicy::default()
        };
        let resolution = resolve_displays(&profile, &connected, &policy);
        assert!(!resolution.should_apply);
        assert_eq!(resolution.missing.len(), 1);
    }
}
