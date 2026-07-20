// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Local animated-image and video metadata probing with bounded poster extraction.

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use easel_core::{AssetId, FrameRate, MediaDimensions, MediaMetadata};
use easel_render::{POSTER_MAX_EDGE, atomic_write_png, render_poster};
use image::AnimationDecoder;
use image::codecs::gif::GifDecoder;
use serde::Deserialize;
use thiserror::Error;

/// Supported still-image file extensions for library indexing.
const STILL_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"];

/// Animated-image extensions indexed as live-surface media.
const ANIMATED_EXTENSIONS: &[&str] = &["gif"];

/// Video container extensions probed when `ffprobe` is available.
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "webm", "mkv", "mov", "m4v"];

/// Returns whether `extension` is a still-image type indexed by Easel.
#[must_use]
pub fn still_image_extension(extension: &str) -> bool {
    let lower = extension.to_ascii_lowercase();
    STILL_EXTENSIONS.iter().any(|candidate| *candidate == lower)
}

/// Returns whether `extension` is a local media type Easel indexes.
#[must_use]
pub fn local_media_extension(extension: &str) -> bool {
    still_image_extension(extension)
        || animated_image_extension(extension)
        || video_extension(extension)
}

/// Returns whether `extension` is an animated image container.
#[must_use]
pub fn animated_image_extension(extension: &str) -> bool {
    let lower = extension.to_ascii_lowercase();
    ANIMATED_EXTENSIONS
        .iter()
        .any(|candidate| *candidate == lower)
}

/// Returns whether `extension` is a video container probed via `ffprobe`.
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
    if video_extension(extension) {
        return probe_video(path);
    }
    if crate::probe::still_image_extension(extension) {
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
    std::fs::create_dir_all(posters_dir)?;
    let dest = poster_path_for_asset(posters_dir, asset_id);
    if animated_image_extension(
        source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default(),
    ) {
        let poster = render_poster(source, POSTER_MAX_EDGE)?;
        atomic_write_png(&dest, &poster)?;
        return Ok(Some(dest));
    }
    if extract_video_poster(source, &dest, POSTER_MAX_EDGE)? {
        return Ok(Some(dest));
    }
    Ok(None)
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

fn probe_video(path: &Path) -> Option<MediaMetadata> {
    let info = ffprobe_video(path)?;
    Some(MediaMetadata::Video {
        dimensions: info.dimensions,
        duration_ms: info.duration_ms,
        frame_rate: info.frame_rate,
        container: info.container,
        video_codec: info.video_codec,
        has_audio: info.has_audio,
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

struct FfprobeVideo {
    dimensions: MediaDimensions,
    duration_ms: Option<u64>,
    frame_rate: Option<FrameRate>,
    container: Option<String>,
    video_codec: Option<String>,
    has_audio: bool,
}

fn ffprobe_video(path: &Path) -> Option<FfprobeVideo> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_streams",
            "-show_format",
        ])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let parsed: FfprobeJson = serde_json::from_slice(&output.stdout).ok()?;
    let video = parsed
        .streams
        .iter()
        .find(|stream| stream.codec_type == "video")?;
    let width = video.width?;
    let height = video.height?;
    if width == 0 || height == 0 {
        return None;
    }
    let has_audio = parsed
        .streams
        .iter()
        .any(|stream| stream.codec_type == "audio");
    let duration_ms = parsed
        .format
        .as_ref()
        .and_then(|format| format.duration.as_deref())
        .and_then(|value| value.parse::<f64>().ok())
        .map(|seconds| {
            let millis = (seconds * 1000.0).round().max(0.0);
            if !millis.is_finite() {
                return 0;
            }
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            {
                millis.min(u64::MAX as f64) as u64
            }
        });
    let frame_rate = video.avg_frame_rate.as_deref().and_then(parse_frame_rate);
    Some(FfprobeVideo {
        dimensions: MediaDimensions { width, height },
        duration_ms,
        frame_rate,
        container: parsed
            .format
            .as_ref()
            .and_then(|format| format.format_name.clone()),
        video_codec: video.codec_name.clone(),
        has_audio,
    })
}

fn parse_frame_rate(value: &str) -> Option<FrameRate> {
    let (numer, denom) = value.split_once('/')?;
    let numerator = numer.trim().parse().ok()?;
    let denominator = denom.trim().parse().ok()?;
    if numerator == 0 || denominator == 0 {
        return None;
    }
    Some(FrameRate {
        numerator,
        denominator,
    })
}

fn extract_video_poster(source: &Path, dest: &Path, max_edge: u32) -> Result<bool, ProbeError> {
    if Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_err()
    {
        return Ok(false);
    }
    let scale = format!("scale={max_edge}:{max_edge}:force_original_aspect_ratio=decrease");
    let mut child = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-ss", "0", "-i"])
        .arg(source)
        .args(["-frames:v", "1", "-vf", &scale, "-update", "1"])
        .arg(dest)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| ProbeError::Poster(error.to_string()))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| ProbeError::Poster(error.to_string()))?
        {
            return Ok(status.success() && dest.is_file());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[derive(Debug, Deserialize)]
struct FfprobeJson {
    streams: Vec<FfprobeStream>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_type: String,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    format_name: Option<String>,
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
}
