// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Local still-image decode with orientation and resource limits.

use std::path::Path;

use easel_core::NativePixelSize;
use image::{DynamicImage, ImageDecoder, ImageError, ImageReader, RgbaImage};
use thiserror::Error;

/// Maximum accepted edge length in pixels.
pub const MAX_EDGE_PIXELS: u32 = 16_384;

/// Maximum accepted pixel count (64 megapixels).
pub const MAX_TOTAL_PIXELS: u64 = 64_000_000;

/// Decoded, oriented source image ready for resampling.
#[derive(Clone, Debug)]
pub struct DecodedImage {
    /// RGBA pixel buffer after orientation normalization.
    pub pixels: RgbaImage,
}

impl DecodedImage {
    /// Oriented source dimensions.
    #[must_use]
    pub fn size(&self) -> NativePixelSize {
        NativePixelSize {
            width: self.pixels.width(),
            height: self.pixels.height(),
        }
    }
}

/// Loads a local still image while enforcing decode limits.
pub fn decode_still(path: &Path) -> Result<DecodedImage, DecodeError> {
    if !path.exists() {
        return Err(DecodeError::MissingFile(path.to_path_buf()));
    }

    let reader = ImageReader::open(path)
        .map_err(|error| DecodeError::Io {
            path: path.to_path_buf(),
            source: error,
        })?
        .with_guessed_format()
        .map_err(|error| DecodeError::Io {
            path: path.to_path_buf(),
            source: error,
        })?;

    let mut decoder = reader.into_decoder().map_err(DecodeError::Image)?;

    let (width, height) = decoder.dimensions();
    enforce_limits(width, height)?;

    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut dynamic = DynamicImage::from_decoder(decoder).map_err(DecodeError::Image)?;
    dynamic.apply_orientation(orientation);

    let pixels = dynamic.to_rgba8();
    enforce_limits(pixels.width(), pixels.height())?;

    Ok(DecodedImage { pixels })
}

fn enforce_limits(width: u32, height: u32) -> Result<(), DecodeError> {
    if width == 0 || height == 0 {
        return Err(DecodeError::EmptyImage);
    }
    if width > MAX_EDGE_PIXELS || height > MAX_EDGE_PIXELS {
        return Err(DecodeError::LimitExceeded {
            width,
            height,
            max_edge: MAX_EDGE_PIXELS,
            max_pixels: MAX_TOTAL_PIXELS,
        });
    }
    let total = u64::from(width).saturating_mul(u64::from(height));
    if total > MAX_TOTAL_PIXELS {
        return Err(DecodeError::LimitExceeded {
            width,
            height,
            max_edge: MAX_EDGE_PIXELS,
            max_pixels: MAX_TOTAL_PIXELS,
        });
    }
    Ok(())
}

/// Still-image decode failure.
#[derive(Debug, Error)]
pub enum DecodeError {
    /// Path does not exist.
    #[error("image file does not exist: {0}")]
    MissingFile(std::path::PathBuf),
    /// Filesystem failure while opening or probing the image.
    #[error("failed to read image {path}: {source}")]
    Io {
        /// Path being read.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Decoder rejected the container or pixel data.
    #[error(transparent)]
    Image(#[from] ImageError),
    /// Source dimensions were empty.
    #[error("decoded image has empty dimensions")]
    EmptyImage,
    /// Source exceeded configured resource limits.
    #[error(
        "image {width}×{height} exceeds decode limits (max edge {max_edge}, max pixels {max_pixels})"
    )]
    LimitExceeded {
        /// Observed width.
        width: u32,
        /// Observed height.
        height: u32,
        /// Configured max edge.
        max_edge: u32,
        /// Configured max pixel count.
        max_pixels: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use std::path::PathBuf;

    #[test]
    fn oversize_limits_are_rejected() {
        assert!(matches!(
            enforce_limits(MAX_EDGE_PIXELS + 1, 10),
            Err(DecodeError::LimitExceeded { .. })
        ));
    }

    #[test]
    fn missing_file_is_reported() {
        let path = PathBuf::from("/tmp/easel-missing-source-does-not-exist.png");
        assert!(matches!(
            decode_still(&path),
            Err(DecodeError::MissingFile(_))
        ));
    }

    #[test]
    fn small_png_decodes() {
        let dir = std::env::temp_dir().join("easel-decode-test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("tiny.png");
        let image = RgbImage::from_pixel(4, 3, Rgb([10, 20, 30]));
        image.save(&path).expect("save");
        let decoded = decode_still(&path).expect("decode");
        assert_eq!(decoded.size().width, 4);
        assert_eq!(decoded.size().height, 3);
    }
}
