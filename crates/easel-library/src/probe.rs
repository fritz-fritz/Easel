// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Local media probing with bounded poster extraction (pure Rust).
//!
//! Stage 6.2 indexes still images and GIF animated images via the `image` crate.
//! Video containers are recognized but not probed yet — decoding and posters will
//! use Qt Multimedia (no external `ffmpeg`/`ffprobe` dependency).

use std::io::BufReader;
use std::path::{Path, PathBuf};

use easel_core::{AssetId, MediaDimensions, MediaMetadata};
use easel_render::{POSTER_MAX_EDGE, atomic_write_png, render_poster};
use image::AnimationDecoder;
use image::codecs::gif::GifDecoder;
use thiserror::Error;

/// Supported still-image file extensions for library indexing.
const STILL_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"];

/// Animated-image extensions indexed as live-surface media.
const ANIMATED_EXTENSIONS: &[&str] = &["gif"];

/// Video container extensions reserved for a future Qt Multimedia probe.
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "webm", "mkv", "mov", "m4v"];

/// Returns whether `extension` is a still-image type indexed by Easel.
#[must_use]
pub fn still_image_extension(extension: &str) -> bool {
    let lower = extension.to_ascii_lowercase();
    STILL_EXTENSIONS.iter().any(|candidate| *candidate == lower)
}

/// Returns whether `extension` is a local media type Easel indexes today.
#[must_use]
pub fn local_media_extension(extension: &str) -> bool {
    still_image_extension(extension) || animated_image_extension(extension)
}

/// Returns whether `extension` is an animated image container.
#[must_use]
pub fn animated_image_extension(extension: &str) -> bool {
    let lower = extension.to_ascii_lowercase();
    ANIMATED_EXTENSIONS
        .iter()
        .any(|candidate| *candidate == lower)
}

/// Returns whether `extension` is a known video container.
///
/// Video files are not indexed yet; metadata and posters will come from Qt
/// Multimedia rather than spawning `ffmpeg`/`ffprobe`.
#[must_use]
pub fn video_extension(extension: &str) -> bool {
    let lower = extension.to_ascii_lowercase();
    VIDEO_EXTENSIONS.iter().any(|candidate| *candidate == lower)
}

/// Probes decoder-visible metadata for a local media file.
#[must_use]
pub fn probe_local_media(path: &Path) -> Option<MediaMetadata> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if animated_image_extension(extension) {
        return probe_animated_image(path);
    }
    if still_image_extension(extension) {
        return probe_still_image(path);
    }
    None
}

/// Returns the poster PNG path for `asset_id` under `posters_dir`.
#[must_use]
pub fn poster_path_for_asset(posters_dir: &Path, asset_id: AssetId) -> PathBuf {
    posters_dir.join(format!("{}.png", asset_id.to_hyphenated_string()))
}

/// Writes a bounded poster PNG when the source requires a live surface.
pub fn write_poster_for_asset(
    source: &Path,
    asset_id: AssetId,
    posters_dir: &Path,
) -> Result<Option<PathBuf>, ProbeError> {
    let metadata = probe_local_media(source).ok_or(ProbeError::Unsupported)?;
    if !metadata.requires_live_surface() {
        return Ok(None);
    }
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !animated_image_extension(extension) {
        return Ok(None);
    }
    std::fs::create_dir_all(posters_dir)?;
    let dest = poster_path_for_asset(posters_dir, asset_id);
    let poster = render_poster(source, POSTER_MAX_EDGE)?;
    atomic_write_png(&dest, &poster)?;
    Ok(Some(dest))
}

/// Media probe or poster extraction failure.
#[derive(Debug, Error)]
pub enum ProbeError {
    /// The file type is not a supported local media container.
    #[error("unsupported local media")]
    Unsupported,
    /// Poster rendering failed.
    #[error("poster extraction failed: {0}")]
    Poster(String),
    /// Filesystem error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<easel_render::PosterError> for ProbeError {
    fn from(error: easel_render::PosterError) -> Self {
        Self::Poster(error.to_string())
    }
}

impl From<easel_render::RasterError> for ProbeError {
    fn from(error: easel_render::RasterError) -> Self {
        Self::Poster(error.to_string())
    }
}

fn probe_still_image(path: &Path) -> Option<MediaMetadata> {
    let dimensions = image_dimensions(path)?;
    Some(MediaMetadata::StillImage { dimensions })
}

fn probe_animated_image(path: &Path) -> Option<MediaMetadata> {
    let dimensions = image_dimensions(path)?;
    let (frame_count, duration_ms) = gif_timing(path).unwrap_or((None, None));
    Some(MediaMetadata::AnimatedImage {
        dimensions,
        duration_ms,
        frame_count,
    })
}

fn image_dimensions(path: &Path) -> Option<MediaDimensions> {
    let (width, height) = image::image_dimensions(path).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some(MediaDimensions { width, height })
}

fn gif_timing(path: &Path) -> Option<(Option<u32>, Option<u64>)> {
    const MAX_FRAMES: u32 = 256;
    let file = std::fs::File::open(path).ok()?;
    let decoder = GifDecoder::new(BufReader::new(file)).ok()?;
    let mut frame_count = 0u32;
    let mut duration_ms = 0u64;
    for frame in decoder.into_frames().flatten().take(MAX_FRAMES as usize) {
        frame_count = frame_count.saturating_add(1);
        let delay = frame.delay();
        let (numer, denom) = delay.numer_denom_ms();
        let numer = u64::from(numer);
        let denom = u64::from(denom.max(1));
        duration_ms = duration_ms.saturating_add(numer / denom);
    }
    if frame_count == 0 {
        return None;
    }
    Some((
        Some(frame_count),
        if duration_ms > 0 {
            Some(duration_ms)
        } else {
            None
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Frame, Rgba, RgbaImage};
    use std::fs::File;
    use uuid::Uuid;

    fn write_gif(path: &Path, width: u32, height: u32, delay_ms: u32) {
        let buffer = RgbaImage::from_pixel(width, height, Rgba([4, 5, 6, 255]));
        let frame = Frame::from_parts(buffer, 0, 0, Delay::from_numer_denom_ms(delay_ms, 1));
        let file = File::create(path).unwrap();
        let mut encoder = GifEncoder::new(file);
        encoder.encode_frame(frame).unwrap();
    }

    #[test]
    fn probes_gif_as_animated_image() {
        let path = std::env::temp_dir().join(format!("easel-probe-gif-{}.gif", Uuid::new_v4()));
        write_gif(&path, 32, 24, 100);
        let metadata = probe_local_media(&path).expect("gif metadata");
        match metadata {
            MediaMetadata::AnimatedImage {
                dimensions,
                frame_count,
                ..
            } => {
                assert_eq!(dimensions.width, 32);
                assert_eq!(dimensions.height, 24);
                assert_eq!(frame_count, Some(1));
            }
            other => panic!("expected animated image, got {other:?}"),
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn writes_poster_for_gif() {
        let root = std::env::temp_dir().join(format!("easel-probe-poster-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("clip.gif");
        write_gif(&path, 64, 48, 50);
        let asset_id = AssetId::new();
        let posters = root.join("posters");
        let written = write_poster_for_asset(&path, asset_id, &posters)
            .unwrap()
            .expect("poster path");
        assert!(written.is_file());
        let (width, height) = image::image_dimensions(&written).unwrap();
        assert!(width <= POSTER_MAX_EDGE);
        assert!(height <= POSTER_MAX_EDGE);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn video_extensions_are_recognized_but_not_indexed() {
        assert!(video_extension("mp4"));
        assert!(!local_media_extension("mp4"));
        let path = std::env::temp_dir().join(format!("easel-probe-video-{}.mp4", Uuid::new_v4()));
        std::fs::write(&path, b"not a real video").unwrap();
        assert!(probe_local_media(&path).is_none());
        let _ = std::fs::remove_file(path);
    }
}
