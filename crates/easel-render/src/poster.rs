// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Bounded first-frame extraction for library thumbnails and live posters.

use std::path::Path;

use image::RgbaImage;
use thiserror::Error;

use crate::decode::{DecodeError, decode_still};
use crate::resize::resize_lanczos3;

/// Default maximum edge for library grid thumbnails and startup posters.
pub const POSTER_MAX_EDGE: u32 = 512;

/// Renders a downscaled first frame suitable for thumbnails and fallback posters.
pub fn render_poster(path: &Path, max_edge: u32) -> Result<RgbaImage, PosterError> {
    if max_edge == 0 {
        return Err(PosterError::InvalidMaxEdge);
    }
    let decoded = decode_still(path)?;
    let width = decoded.pixels.width();
    let height = decoded.pixels.height();
    if width <= max_edge && height <= max_edge {
        return Ok(decoded.pixels);
    }
    let scale = f64::from(max_edge) / f64::from(width.max(height));
    let out_width = scale_dimension(width, scale);
    let out_height = scale_dimension(height, scale);
    Ok(resize_lanczos3(&decoded.pixels, out_width, out_height))
}

fn scale_dimension(pixels: u32, scale: f64) -> u32 {
    let scaled = (f64::from(pixels) * scale)
        .round()
        .clamp(1.0, f64::from(u32::MAX));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        scaled as u32
    }
}

/// Poster extraction failure.
#[derive(Debug, Error)]
pub enum PosterError {
    /// `max_edge` must be non-zero.
    #[error("poster max edge must be greater than zero")]
    InvalidMaxEdge,
    /// Still decode failed.
    #[error(transparent)]
    Decode(#[from] DecodeError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn poster_scales_down_large_still() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "easel-poster-{}-{}.png",
            std::process::id(),
            unique
        ));
        RgbImage::from_pixel(1024, 512, Rgb([1, 2, 3]))
            .save(&path)
            .unwrap();
        let poster = render_poster(&path, 256).unwrap();
        assert!(poster.width() <= 256);
        assert!(poster.height() <= 256);
        let _ = std::fs::remove_file(path);
    }
}
