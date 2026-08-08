// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bounded motion-frame extraction for still-backend slideshows (ADR 0014).

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, RgbaImage};
use thiserror::Error;

use crate::raster::{RasterError, atomic_write_png};

/// Soft cap on extracted GIF frames (hostile/long animations).
pub const MAX_MOTION_FRAMES: usize = 64;

/// Minimum per-frame hold used when the container reports a zero delay.
pub const DEFAULT_FRAME_DELAY_MS: u64 = 100;

/// Floor applied when scheduling still-backend Apply ticks (avoids xfconf thrash).
pub const MIN_SLIDESHOW_DELAY_MS: u64 = 200;

/// One extracted still frame ready for composition / Apply.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MotionFrame {
    /// Absolute PNG path.
    pub path: PathBuf,
    /// Hold duration for this frame (milliseconds), already clamped for slideshow use.
    pub delay_ms: u64,
    /// Media timeline offset at the start of this frame (milliseconds).
    pub media_time_ms: u64,
}

/// Extracts GIF frames to PNGs under `output_dir` for still-backend slideshow Apply.
pub fn extract_gif_frames(
    source: &Path,
    output_dir: &Path,
    max_frames: usize,
) -> Result<Vec<MotionFrame>, FrameExtractError> {
    if max_frames == 0 {
        return Err(FrameExtractError::Empty);
    }
    std::fs::create_dir_all(output_dir)?;
    let file = File::open(source)?;
    let decoder = GifDecoder::new(BufReader::new(file))?;
    let mut frames = Vec::new();
    let mut media_time_ms = 0u64;
    for (index, frame) in decoder.into_frames().enumerate() {
        if index >= max_frames {
            break;
        }
        let frame = frame?;
        let delay = frame.delay();
        let (numer, denom) = delay.numer_denom_ms();
        let raw_ms = u64::from(numer) / u64::from(denom.max(1));
        let delay_ms = raw_ms
            .max(DEFAULT_FRAME_DELAY_MS)
            .max(MIN_SLIDESHOW_DELAY_MS);
        let buffer = frame.into_buffer();
        let path = output_dir.join(format!("frame-{index:04}.png"));
        write_rgba_png(&path, &buffer)?;
        frames.push(MotionFrame {
            path,
            delay_ms,
            media_time_ms,
        });
        media_time_ms = media_time_ms.saturating_add(delay_ms);
    }
    if frames.is_empty() {
        return Err(FrameExtractError::Empty);
    }
    Ok(frames)
}

fn write_rgba_png(path: &Path, image: &RgbaImage) -> Result<(), FrameExtractError> {
    atomic_write_png(path, image).map_err(FrameExtractError::from)
}

/// Motion-frame extraction failure.
#[derive(Debug, Error)]
pub enum FrameExtractError {
    /// Container produced no frames.
    #[error("motion source produced no frames")]
    Empty,
    /// GIF decode failed.
    #[error("gif decode failed: {0}")]
    Gif(#[from] image::ImageError),
    /// Filesystem error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// PNG write failed.
    #[error("raster error: {0}")]
    Raster(#[from] RasterError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Delay, Frame, Rgba, RgbaImage};

    fn write_tiny_gif(path: &Path, delays_ms: &[u32]) {
        use image::codecs::gif::{GifEncoder, Repeat};
        let file = File::create(path).unwrap();
        let mut encoder = GifEncoder::new(file);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for (index, &delay) in delays_ms.iter().enumerate() {
            let red = u8::try_from(index.saturating_mul(40)).unwrap_or(u8::MAX);
            let image = RgbaImage::from_pixel(2, 2, Rgba([red, 0, 0, 255]));
            let frame = Frame::from_parts(image, 0, 0, Delay::from_numer_denom_ms(delay, 1));
            encoder.encode_frame(frame).unwrap();
        }
    }

    #[test]
    fn extracts_gif_frames_with_clamped_delays() {
        let dir = std::env::temp_dir().join(format!(
            "easel-frames-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let gif = dir.join("tiny.gif");
        write_tiny_gif(&gif, &[10, 50]);
        let out = dir.join("out");
        let frames = extract_gif_frames(&gif, &out, MAX_MOTION_FRAMES).unwrap();
        assert_eq!(frames.len(), 2);
        assert!(frames[0].delay_ms >= MIN_SLIDESHOW_DELAY_MS);
        assert!(frames[0].path.is_file());
        assert!(frames[1].path.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
