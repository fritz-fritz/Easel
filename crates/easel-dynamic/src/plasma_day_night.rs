// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Plasma built-in day/night wallpaper packages (`org.kde.image` + KNightTime).
//!
//! Plasma 6.5+ can host light/dark wallpaper packages natively: a folder with
//! `contents/images` and `contents/images_dark` plus `metadata.json`. The OS
//! switches at sunrise/sunset via KNightTime. This is **not** Apple-style dense
//! solar HEIC — only two appearance frames. Dense solar/h24 sets still need the
//! Easel still poller or the community `com.github.zzag.dynamic` plugin.

use std::fs;
use std::path::{Path, PathBuf};

use easel_core::{AppearanceMode, DynamicStillKey, DynamicStillSet};
use easel_render::{RasterError, atomic_write_png};
use image::RgbaImage;
use thiserror::Error;

/// One completed Plasma day/night wallpaper package on disk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlasmaDayNightPackage {
    /// Package root directory (install under `~/.local/share/wallpapers`).
    pub path: PathBuf,
    /// KPlugin Id written into metadata.
    pub plugin_id: String,
    /// Suggested `Image` config value (file URL to the light image).
    pub image_path: PathBuf,
}

/// Writes a Plasma day/night wallpaper package from light + dark RGBA frames.
///
/// Image file names follow Plasma convention: `{width}x{height}.png`.
pub fn write_plasma_day_night_package(
    package_dir: impl AsRef<Path>,
    plugin_id: impl Into<String>,
    name: impl Into<String>,
    light: &RgbaImage,
    dark: &RgbaImage,
    cross_fade: bool,
) -> Result<PlasmaDayNightPackage, PlasmaDayNightError> {
    let package_dir = package_dir.as_ref();
    let plugin_id = plugin_id.into();
    let name = name.into();
    if light.width() == 0 || light.height() == 0 || dark.width() == 0 || dark.height() == 0 {
        return Err(PlasmaDayNightError::EmptyImage);
    }
    if light.width() != dark.width() || light.height() != dark.height() {
        return Err(PlasmaDayNightError::SizeMismatch {
            light: (light.width(), light.height()),
            dark: (dark.width(), dark.height()),
        });
    }

    let file_name = format!("{}x{}.png", light.width(), light.height());
    let images = package_dir.join("contents").join("images");
    let images_dark = package_dir.join("contents").join("images_dark");
    fs::create_dir_all(&images)?;
    fs::create_dir_all(&images_dark)?;

    let light_path = images.join(&file_name);
    let dark_path = images_dark.join(&file_name);
    atomic_write_png(&light_path, light)?;
    atomic_write_png(&dark_path, dark)?;

    let metadata = format!(
        r#"{{
  "KPlugin": {{
    "Id": "{id}",
    "Name": "{name}",
    "License": "Unknown"
  }},
  "X-KDE-CrossFade": {cross_fade}
}}
"#,
        id = escape_json(&plugin_id),
        name = escape_json(&name),
        cross_fade = if cross_fade { "true" } else { "false" },
    );
    fs::write(package_dir.join("metadata.json"), metadata)?;

    Ok(PlasmaDayNightPackage {
        path: package_dir.to_path_buf(),
        plugin_id,
        image_path: light_path,
    })
}

/// Extracts light/dark frames from an appearance-keyed still set + RGBA buffers.
pub fn appearance_frames_from_set(
    set: &DynamicStillSet,
    images: &[RgbaImage],
) -> Result<(RgbaImage, RgbaImage), PlasmaDayNightError> {
    if images.len() != set.frames.len() {
        return Err(PlasmaDayNightError::FrameCountMismatch {
            frames: set.frames.len(),
            images: images.len(),
        });
    }
    let mut light = None;
    let mut dark = None;
    for (frame, image) in set.frames.iter().zip(images.iter()) {
        match frame.key {
            DynamicStillKey::Appearance {
                mode: AppearanceMode::Light,
            } => light = Some(image.clone()),
            DynamicStillKey::Appearance {
                mode: AppearanceMode::Dark,
            } => dark = Some(image.clone()),
            _ => {
                return Err(PlasmaDayNightError::NotAppearanceSet);
            }
        }
    }
    match (light, dark) {
        (Some(l), Some(d)) => Ok((l, d)),
        _ => Err(PlasmaDayNightError::MissingAppearancePair),
    }
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Plasma day/night package failures.
#[derive(Debug, Error)]
pub enum PlasmaDayNightError {
    /// Empty pixel buffer.
    #[error("cannot write an empty Plasma day/night image")]
    EmptyImage,
    /// Light and dark dimensions differ.
    #[error("light {light:?} and dark {dark:?} dimensions must match")]
    SizeMismatch {
        /// Light size.
        light: (u32, u32),
        /// Dark size.
        dark: (u32, u32),
    },
    /// Still set is not appearance-keyed.
    #[error("Plasma day/night packages require an Appearance still set")]
    NotAppearanceSet,
    /// Missing light or dark frame.
    #[error("Appearance still set must include both light and dark frames")]
    MissingAppearancePair,
    /// Frame / image count mismatch.
    #[error("frame count {frames} does not match image count {images}")]
    FrameCountMismatch {
        /// Domain frames.
        frames: usize,
        /// RGBA buffers.
        images: usize,
    },
    /// PNG write failure.
    #[error(transparent)]
    Raster(#[from] RasterError),
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::{AssetId, DynamicStillFrame, DynamicStillSet, ProfileId};
    use image::{Rgba, RgbaImage};

    #[test]
    fn writes_package_layout() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = dir.path().join("Lake");
        let light = RgbaImage::from_pixel(32, 24, Rgba([240, 240, 230, 255]));
        let dark = RgbaImage::from_pixel(32, 24, Rgba([20, 20, 30, 255]));
        let written =
            write_plasma_day_night_package(&pkg, "easel.lake", "Lake", &light, &dark, true)
                .unwrap();
        assert!(pkg.join("metadata.json").is_file());
        assert!(pkg.join("contents/images/32x24.png").is_file());
        assert!(pkg.join("contents/images_dark/32x24.png").is_file());
        assert_eq!(written.image_path, pkg.join("contents/images/32x24.png"));
        let meta = std::fs::read_to_string(pkg.join("metadata.json")).unwrap();
        assert!(meta.contains("\"Id\": \"easel.lake\""));
        assert!(meta.contains("\"X-KDE-CrossFade\": true"));
    }

    #[test]
    fn extracts_appearance_pair() {
        let light_id = AssetId::new();
        let dark_id = AssetId::new();
        let mut set = DynamicStillSet::with_fallback("Apr", ProfileId::new(), light_id);
        set.schedule_kind = easel_core::DynamicScheduleKind::Appearance;
        set.frames = vec![
            DynamicStillFrame {
                source_index: Some(0),
                key: DynamicStillKey::Appearance {
                    mode: AppearanceMode::Light,
                },
                asset_id: light_id,
            },
            DynamicStillFrame {
                source_index: Some(1),
                key: DynamicStillKey::Appearance {
                    mode: AppearanceMode::Dark,
                },
                asset_id: dark_id,
            },
        ];
        let images = vec![
            RgbaImage::from_pixel(8, 8, Rgba([1, 2, 3, 255])),
            RgbaImage::from_pixel(8, 8, Rgba([4, 5, 6, 255])),
        ];
        let (light, dark) = appearance_frames_from_set(&set, &images).unwrap();
        assert_eq!(light.get_pixel(0, 0).0, [1, 2, 3, 255]);
        assert_eq!(dark.get_pixel(0, 0).0, [4, 5, 6, 255]);
    }
}
