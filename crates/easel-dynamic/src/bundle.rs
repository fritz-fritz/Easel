// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Per-display native dynamic bundle planning and encode.
//!
//! Pipeline: plan one package per display → crop every source frame with the
//! existing raster planner → encode Apple/Plasma HEIC with schedule XMP.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use easel_core::{Display, DisplayId, DynamicStillSet};
use easel_render::{
    CompositionSettings, RENDERER_VERSION, RasterError, RenderPlan, RenderPurpose, decode_still,
    render_operation,
};
use image::RgbaImage;
use thiserror::Error;

use crate::encode::{HeicEncodeError, encode_still_set_heic};

/// Native container a backend can host without Easel polling frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeDynamicFormat {
    /// Apple Dynamic Desktop HEIC (`apple_desktop` XMP).
    AppleHeic,
    /// Plasma dynamic wallpaper HEIC/AVIF package (same Apple XMP interchange today).
    PlasmaHeic,
}

/// One planned native package for a single display.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicBundleTarget {
    /// Display receiving the package.
    pub display_id: DisplayId,
    /// Native format to encode.
    pub format: NativeDynamicFormat,
    /// Suggested file name stem (`{set}-{display}`).
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

/// One completed native package on disk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedDynamicBundle {
    /// Display that received this crop package.
    pub display_id: DisplayId,
    /// Absolute path to the encoded HEIC.
    pub path: PathBuf,
    /// Format written.
    pub format: NativeDynamicFormat,
}

/// Plans one native dynamic package per display for backends that can host them.
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

/// Crops every frame for each display and encodes one native HEIC package per display.
///
/// `frame_paths` must align with `set.frames`. Cache keys include arrangement geometry
/// and renderer version so topology changes invalidate packages.
pub fn encode_per_display_bundles(
    set: &DynamicStillSet,
    frame_paths: &[impl AsRef<Path>],
    displays: &[Display],
    composition: &CompositionSettings,
    format: NativeDynamicFormat,
    output_dir: impl AsRef<Path>,
) -> Result<Vec<EncodedDynamicBundle>, BundleEncodeError> {
    if frame_paths.len() != set.frames.len() {
        return Err(BundleEncodeError::FramePathCount {
            frames: set.frames.len(),
            paths: frame_paths.len(),
        });
    }
    let plan = plan_per_display_bundles(set, displays, format)?;
    let output_dir = output_dir.as_ref();
    std::fs::create_dir_all(output_dir)?;

    // display_id → cropped frames in still-set order
    let mut crops: HashMap<DisplayId, Vec<RgbaImage>> = HashMap::new();
    for display in displays {
        crops.insert(display.id, Vec::with_capacity(set.frames.len()));
    }

    let render_plan = RenderPlan::for_purpose(displays, RenderPurpose::StaticWallpaper)?;
    for path in frame_paths {
        let path = path.as_ref();
        let decoded = decode_still(path)?;
        let operations = render_plan.operations(decoded.size(), composition)?;
        let mut by_display: HashMap<DisplayId, RgbaImage> = HashMap::new();
        for operation in operations {
            let canvas = render_operation(&decoded.pixels, &operation)?;
            by_display.insert(operation.display_id, canvas);
        }
        for display in displays {
            let canvas = by_display
                .remove(&display.id)
                .ok_or_else(|| BundleEncodeError::MissingCrop(display.id.to_hyphenated_string()))?;
            crops
                .get_mut(&display.id)
                .expect("crop map initialized")
                .push(canvas);
        }
    }

    let mut encoded = Vec::with_capacity(plan.targets.len());
    for target in &plan.targets {
        let images = crops.remove(&target.display_id).ok_or_else(|| {
            BundleEncodeError::MissingCrop(target.display_id.to_hyphenated_string())
        })?;
        let file_name = format!(
            "v{RENDERER_VERSION}_{}.heic",
            target.file_stem.replace('-', "")
        );
        let out_path = output_dir.join(file_name);
        encode_still_set_heic(set, &images, &out_path)?;
        encoded.push(EncodedDynamicBundle {
            display_id: target.display_id,
            path: out_path,
            format: target.format,
        });
    }
    Ok(encoded)
}

/// Resolves an existing cached native bundle path for a display when present.
#[must_use]
pub fn cached_bundle_path(
    set: &DynamicStillSet,
    display_id: DisplayId,
    output_dir: &Path,
) -> Option<PathBuf> {
    let stem = format!(
        "{}-{}",
        set.id.to_hyphenated_string(),
        display_id.to_hyphenated_string()
    );
    let path = output_dir.join(format!(
        "v{RENDERER_VERSION}_{}.heic",
        stem.replace('-', "")
    ));
    path.is_file().then_some(path)
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

/// Bundle encode failures.
#[derive(Debug, Error)]
pub enum BundleEncodeError {
    /// Planning failed.
    #[error(transparent)]
    Plan(#[from] BundlePlanError),
    /// Frame path count mismatch.
    #[error("frame path count {paths} does not match still-set frames {frames}")]
    FramePathCount {
        /// Domain frames.
        frames: usize,
        /// Paths supplied.
        paths: usize,
    },
    /// A display was missing from crop output.
    #[error("missing crop for display {0}")]
    MissingCrop(String),
    /// Decode failure.
    #[error(transparent)]
    Decode(#[from] easel_render::DecodeError),
    /// Raster / crop failure.
    #[error(transparent)]
    Raster(#[from] RasterError),
    /// Render plan failure.
    #[error(transparent)]
    RenderPlan(#[from] easel_render::RenderPlanError),
    /// HEIC encode failure.
    #[error(transparent)]
    Encode(#[from] HeicEncodeError),
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::{
        AssetId, DynamicStillSet, FitMode, LayoutMode, ProfileId, all_layout_fixtures,
    };
    use easel_render::CompositionSettings;
    use image::{Rgba, RgbaImage};

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

    #[test]
    fn encodes_per_display_bundles_from_png_frames() {
        let dir = tempfile::tempdir().unwrap();
        let asset = AssetId::new();
        let set = DynamicStillSet::default_time_of_day("Day", ProfileId::new(), asset).unwrap();
        let mut frame_paths = Vec::new();
        for (index, color) in [[200u8, 80, 40, 255], [40, 120, 200, 255], [20, 20, 40, 255]]
            .into_iter()
            .enumerate()
        {
            let path = dir.path().join(format!("frame-{index}.png"));
            RgbaImage::from_pixel(64, 48, Rgba(color))
                .save(&path)
                .unwrap();
            frame_paths.push(path);
        }
        let displays = vec![all_layout_fixtures()[0].1.displays[0].clone()];
        // Shrink fixture display so encode stays cheap.
        let mut displays = displays;
        displays[0].native_pixels.width = 32;
        displays[0].native_pixels.height = 24;

        let out = dir.path().join("bundles");
        let encoded = encode_per_display_bundles(
            &set,
            &frame_paths,
            &displays,
            &CompositionSettings {
                fit_mode: FitMode::Cover,
                layout_mode: LayoutMode::Digital,
                zoom: 1.0,
                focal_x: 0.5,
                focal_y: 0.5,
            },
            NativeDynamicFormat::AppleHeic,
            &out,
        )
        .expect("encode bundles");
        assert_eq!(encoded.len(), 1);
        assert!(encoded[0].path.is_file());
        assert!(cached_bundle_path(&set, displays[0].id, &out).is_some());
    }
}
