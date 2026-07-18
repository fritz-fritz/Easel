// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Display hotplug policy for wallpaper application and recovery.

use serde::{Deserialize, Serialize};

use crate::{Display, DisplayGroup, DisplayId};

/// How Easel behaves when expected outputs are missing after a topology change.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingOutputPolicy {
    /// Apply to the displays that are still present; skip missing members.
    #[default]
    SkipMissing,
    /// Do not apply until every expected display is present again.
    PauseUntilRestored,
    /// Apply only when at least one expected display remains; otherwise pause.
    RequireAny,
}

/// Result of resolving a display group against the live arrangement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HotplugResolution {
    /// Displays from the group that are currently connected.
    pub present: Vec<DisplayId>,
    /// Group members that are not in the live arrangement.
    pub missing: Vec<DisplayId>,
    /// Whether the active policy permits applying wallpaper now.
    pub may_apply: bool,
    /// Explainable reason for `may_apply`.
    pub reason: String,
}

/// Resolves which group members are present and whether apply is allowed.
#[must_use]
pub fn resolve_hotplug(
    group: &DisplayGroup,
    live_displays: &[Display],
    policy: MissingOutputPolicy,
) -> HotplugResolution {
    let live: std::collections::HashSet<DisplayId> =
        live_displays.iter().map(|display| display.id).collect();
    let mut present = Vec::new();
    let mut missing = Vec::new();
    for id in &group.displays {
        if live.contains(id) {
            present.push(*id);
        } else {
            missing.push(*id);
        }
    }

    let (may_apply, reason) = match policy {
        MissingOutputPolicy::SkipMissing => {
            if present.is_empty() {
                (
                    false,
                    "no expected displays are connected; skipping apply".to_owned(),
                )
            } else if missing.is_empty() {
                (true, "all expected displays are connected".to_owned())
            } else {
                (
                    true,
                    format!(
                        "applying to {} present display(s); {} missing under skip-missing policy",
                        present.len(),
                        missing.len()
                    ),
                )
            }
        }
        MissingOutputPolicy::PauseUntilRestored => {
            if missing.is_empty() {
                (true, "all expected displays are connected".to_owned())
            } else {
                (
                    false,
                    format!(
                        "paused until {} missing display(s) reconnect",
                        missing.len()
                    ),
                )
            }
        }
        MissingOutputPolicy::RequireAny => {
            if present.is_empty() {
                (
                    false,
                    "no expected displays remain; pausing under require-any policy".to_owned(),
                )
            } else {
                (
                    true,
                    format!(
                        "applying to {} of {} expected display(s)",
                        present.len(),
                        group.displays.len()
                    ),
                )
            }
        }
    };

    HotplugResolution {
        present,
        missing,
        may_apply,
        reason,
    }
}

/// Filters a live arrangement down to the displays listed in `present`.
#[must_use]
pub fn filter_displays(live_displays: &[Display], present: &[DisplayId]) -> Vec<Display> {
    let wanted: std::collections::HashSet<DisplayId> = present.iter().copied().collect();
    live_displays
        .iter()
        .filter(|display| wanted.contains(&display.id))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::{BezelInsets, PhysicalSizeSource};
    use crate::{
        DisplayId, LogicalRect, Millimeters, NativePixelSize, PhysicalPoint, PhysicalSize,
        ScaleFactor,
    };

    fn display(id: u128) -> Display {
        Display {
            id: DisplayId::from_u128(id),
            connector_name: Some(format!("DP-{id}")),
            manufacturer: None,
            model: None,
            serial: None,
            logical_rect: LogicalRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            native_pixels: NativePixelSize {
                width: 1920,
                height: 1080,
            },
            scale_factor: ScaleFactor::default(),
            physical_size: PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(280.0),
            },
            physical_size_source: PhysicalSizeSource::Detected,
            physical_origin: PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
            bezel: BezelInsets::default(),
            rotation_degrees: 0,
        }
    }

    #[test]
    fn pause_until_restored_blocks_partial() {
        let group = DisplayGroup::new(
            "Desk",
            vec![DisplayId::from_u128(1), DisplayId::from_u128(2)],
        );
        let live = vec![display(1)];
        let resolution = resolve_hotplug(&group, &live, MissingOutputPolicy::PauseUntilRestored);
        assert!(!resolution.may_apply);
        assert_eq!(resolution.missing, vec![DisplayId::from_u128(2)]);
    }

    #[test]
    fn skip_missing_allows_partial() {
        let group = DisplayGroup::new(
            "Desk",
            vec![DisplayId::from_u128(1), DisplayId::from_u128(2)],
        );
        let live = vec![display(1)];
        let resolution = resolve_hotplug(&group, &live, MissingOutputPolicy::SkipMissing);
        assert!(resolution.may_apply);
        assert_eq!(resolution.present, vec![DisplayId::from_u128(1)]);
    }
}
