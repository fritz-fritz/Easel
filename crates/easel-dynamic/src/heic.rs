// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Decode Apple dynamic HEIC containers into frames + schedule metadata.

use std::fs;
use std::path::{Path, PathBuf};

use easel_core::{
    DynamicScheduleKind, DynamicStillFrame, DynamicStillKey, DynamicStillSet, ProfileId,
};
use image::RgbaImage;
use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
use thiserror::Error;

use crate::metadata::{
    AppleDesktopMetadata, AppleMetadataFlavor, MetadataError, parse_apple_desktop_from_xmp,
    scrape_xmp_packet,
};

/// One decoded frame from a dynamic HEIC.
#[derive(Clone, Debug)]
pub struct ImportedDynamicFrame {
    /// Image index inside the HEIC.
    pub index: u32,
    /// Domain key derived from Apple metadata.
    pub key: DynamicStillKey,
    /// Decoded RGBA pixels.
    pub image: RgbaImage,
}

/// Fully imported dynamic desktop package.
#[derive(Clone, Debug)]
pub struct ImportedDynamicDesktop {
    /// Source file path.
    pub source_path: PathBuf,
    /// Metadata flavor.
    pub flavor: AppleMetadataFlavor,
    /// Schedule kind for the resulting still set.
    pub schedule_kind: DynamicScheduleKind,
    /// Decoded frames with keys.
    pub frames: Vec<ImportedDynamicFrame>,
}

impl ImportedDynamicDesktop {
    /// Builds a `DynamicStillSet` shell; caller assigns `asset_id`s after library upsert.
    pub fn into_still_set_template(
        &self,
        name: impl Into<String>,
        profile_id: ProfileId,
        asset_ids: &[easel_core::AssetId],
    ) -> Result<DynamicStillSet, HeicImportError> {
        if asset_ids.len() != self.frames.len() || asset_ids.is_empty() {
            return Err(HeicImportError::AssetCountMismatch {
                frames: self.frames.len(),
                assets: asset_ids.len(),
            });
        }
        let fallback = asset_ids[0];
        let mut set = DynamicStillSet::with_fallback(name, profile_id, fallback);
        set.schedule_kind = self.schedule_kind;
        set.source_package_path = Some(self.source_path.display().to_string());
        set.request_cross_fade = true;
        set.frames = self
            .frames
            .iter()
            .zip(asset_ids.iter().copied())
            .map(|(frame, asset_id)| DynamicStillFrame {
                source_index: Some(frame.index),
                key: frame.key,
                asset_id,
            })
            .collect();
        set.validate()?;
        Ok(set)
    }
}

/// Imports an Apple (or Plasma-compatible) dynamic HEIC from disk.
pub fn import_dynamic_heic(
    path: impl AsRef<Path>,
) -> Result<ImportedDynamicDesktop, HeicImportError> {
    let path = path.as_ref();
    let bytes = fs::read(path)?;
    let xmp = scrape_xmp_packet(&bytes).ok_or(HeicImportError::MissingXmp)?;
    let meta = parse_apple_desktop_from_xmp(&xmp)?;
    let frames = decode_heic_frames(path, &meta)?;
    Ok(ImportedDynamicDesktop {
        source_path: path.to_path_buf(),
        flavor: meta.flavor,
        schedule_kind: meta.schedule_kind,
        frames,
    })
}

fn decode_heic_frames(
    path: &Path,
    meta: &AppleDesktopMetadata,
) -> Result<Vec<ImportedDynamicFrame>, HeicImportError> {
    let lib = LibHeif::new();
    let ctx = HeifContext::read_from_file(path.to_string_lossy().as_ref())
        .map_err(|error| HeicImportError::Heif(error.to_string()))?;
    let handles = ctx.top_level_image_handles();
    if handles.is_empty() {
        return Err(HeicImportError::NoImages);
    }

    let mut frames = Vec::new();
    for (index, handle) in handles.into_iter().enumerate() {
        let index = u32::try_from(index).unwrap_or(u32::MAX);
        let Some(key) = meta.keys_by_index.get(&index).copied() else {
            // Appearance sets only key two indices; skip unkeyed extras.
            continue;
        };
        let image = lib
            .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
            .map_err(|error| HeicImportError::Heif(error.to_string()))?;
        let planes = image
            .planes()
            .interleaved
            .ok_or(HeicImportError::MissingPixels)?;
        let width = planes.width;
        let height = planes.height;
        let stride = planes.stride;
        let data = planes.data;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height {
            let start = row as usize * stride;
            let end = start + width as usize * 4;
            rgba.extend_from_slice(data.get(start..end).ok_or(HeicImportError::MissingPixels)?);
        }
        let image =
            RgbaImage::from_raw(width, height, rgba).ok_or(HeicImportError::MissingPixels)?;
        frames.push(ImportedDynamicFrame { index, key, image });
    }
    if frames.is_empty() {
        return Err(HeicImportError::EmptySchedule);
    }
    frames.sort_by_key(|frame| frame.index);
    Ok(frames)
}

/// HEIC import failures.
#[derive(Debug, Error)]
pub enum HeicImportError {
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// XMP packet missing from the container.
    #[error("dynamic HEIC is missing an XMP packet")]
    MissingXmp,
    /// Apple metadata parse failure.
    #[error(transparent)]
    Metadata(#[from] MetadataError),
    /// libheif failure.
    #[error("libheif error: {0}")]
    Heif(String),
    /// Container had no top-level images.
    #[error("dynamic HEIC contains no images")]
    NoImages,
    /// Decoded image lacked interleaved RGBA pixels.
    #[error("decoded HEIC frame missing RGBA pixels")]
    MissingPixels,
    /// Metadata produced no keyed frames.
    #[error("dynamic HEIC schedule produced no keyed frames")]
    EmptySchedule,
    /// Caller supplied the wrong number of asset ids.
    #[error("asset count {assets} does not match frame count {frames}")]
    AssetCountMismatch {
        /// Number of decoded frames.
        frames: usize,
        /// Number of asset ids supplied.
        assets: usize,
    },
    /// Domain validation failure after import.
    #[error(transparent)]
    DynamicStill(#[from] easel_core::DynamicStillError),
}
