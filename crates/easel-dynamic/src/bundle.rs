// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Per-display native dynamic bundle planning.
//!
//! Encoding is platform-specific; this module produces the plan Easel should
//! execute: one native package per display containing every cropped frame plus
//! the original schedule metadata.

use easel_core::{Display, DisplayId, DynamicStillSet};
use thiserror::Error;

/// Native container a backend can host without Easel polling frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeDynamicFormat {
    /// Apple Dynamic Desktop HEIC (`apple_desktop` XMP).
    AppleHeic,
    /// Plasma dynamic wallpaper HEIC/AVIF package.
    PlasmaHeic,
}

/// One planned native package for a single display.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicBundleTarget {
    /// Display receiving the package.
    pub display_id: DisplayId,
    /// Native format to encode.
    pub format: NativeDynamicFormat,
    /// Suggested file name stem (`{display}-{set}`).
    pub file_stem: String,
}

/// Full encode plan derived from a still set and active displays.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicBundlePlan {
    /// Still set being packaged.
    pub still_set_id: easel_core::DynamicStillSetId,
    /// Number of frames that must be cropped into every package.
    pub frame_count: usize,
    /// Per-display outputs.
    pub targets: Vec<DynamicBundleTarget>,
}

/// Plans one native dynamic package per display for backends that can host them.
///
/// Does not encode bytes; callers crop each source frame with the existing raster
/// planner, then pass the crops to a platform encoder.
pub fn plan_per_display_bundles(
    set: &DynamicStillSet,
    displays: &[Display],
    format: NativeDynamicFormat,
) -> Result<DynamicBundlePlan, BundlePlanError> {
    if displays.is_empty() {
        return Err(BundlePlanError::NoDisplays);
    }
    if set.frames.is_empty() {
        return Err(BundlePlanError::NoFrames);
    }
    let targets = displays
        .iter()
        .map(|display| DynamicBundleTarget {
            display_id: display.id,
            format,
            file_stem: format!(
                "{}-{}",
                set.id.to_hyphenated_string(),
                display.id.to_hyphenated_string()
            ),
        })
        .collect();
    Ok(DynamicBundlePlan {
        still_set_id: set.id,
        frame_count: set.frames.len(),
        targets,
    })
}

/// Bundle planning failures.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BundlePlanError {
    /// No displays were provided.
    #[error("cannot plan dynamic bundles without displays")]
    NoDisplays,
    /// Still set has no keyed frames.
    #[error("cannot plan dynamic bundles for an empty still set")]
    NoFrames,
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::{AssetId, DynamicStillSet, ProfileId, all_layout_fixtures};

    #[test]
    fn plans_one_target_per_display() {
        let asset = AssetId::new();
        let set = DynamicStillSet::default_hourly("Day", ProfileId::new(), asset).unwrap();
        let displays = all_layout_fixtures()[0].1.displays.clone();
        let plan =
            plan_per_display_bundles(&set, &displays, NativeDynamicFormat::AppleHeic).unwrap();
        assert_eq!(plan.frame_count, 24);
        assert_eq!(plan.targets.len(), displays.len());
        assert!(
            plan.targets
                .iter()
                .all(|target| target.format == NativeDynamicFormat::AppleHeic)
        );
    }
}
