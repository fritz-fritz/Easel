// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Encode domain still frames into Apple-style dynamic HEIC packages.

use std::collections::HashMap;
use std::path::Path;

use easel_core::{DynamicStillKey, DynamicStillSet};
use image::RgbaImage;
use libheif_rs::{
    Channel, ColorSpace, CompressionFormat, EncoderQuality, HeifContext, Image, LibHeif, RgbChroma,
};
use thiserror::Error;

use crate::metadata::{
    AppleMetadataFlavor, MetadataError, apple_keys_from_still_set, build_apple_xmp,
};

/// One RGBA frame paired with its Apple image index and domain key.
#[derive(Clone, Debug)]
pub struct EncodeFrame {
    /// Top-level image index inside the HEIC (matches `source_index`).
    pub index: u32,
    /// Domain key written into Apple metadata.
    pub key: DynamicStillKey,
    /// Pixels to encode (RGB planes; alpha discarded).
    pub image: RgbaImage,
}

/// Encodes a multi-image Apple dynamic HEIC with XMP schedule metadata.
pub fn encode_dynamic_heic(
    frames: &[EncodeFrame],
    flavor: AppleMetadataFlavor,
    output: impl AsRef<Path>,
) -> Result<(), HeicEncodeError> {
    if frames.is_empty() {
        return Err(HeicEncodeError::NoFrames);
    }
    // Decode matches metadata keys against top-level image enumeration order (0..N-1),
    // so XMP indices must be contiguous encode ordinals — not arbitrary `EncodeFrame.index`.
    let mut ordered = frames.to_vec();
    ordered.sort_by_key(|frame| frame.index);
    let mut keys_by_index = HashMap::new();
    for (encode_index, frame) in ordered.iter().enumerate() {
        let index = u32::try_from(encode_index).unwrap_or(u32::MAX);
        keys_by_index.insert(index, frame.key);
    }
    let xmp = build_apple_xmp(flavor, &keys_by_index)?;

    let lib = LibHeif::new();
    let mut ctx = HeifContext::new().map_err(|error| HeicEncodeError::Heif(error.to_string()))?;
    let mut encoder = open_encoder(&lib)?;
    encoder
        .set_quality(EncoderQuality::Lossy(90))
        .map_err(|error| HeicEncodeError::Heif(error.to_string()))?;

    let mut primary_handle = None;
    for frame in &ordered {
        let image = rgba_to_heif_image(&frame.image)?;
        let handle = ctx
            .encode_image(&image, &mut encoder, None)
            .map_err(|error| HeicEncodeError::Heif(error.to_string()))?;
        if primary_handle.is_none() {
            primary_handle = Some(handle);
        }
    }
    let primary = primary_handle.ok_or(HeicEncodeError::NoFrames)?;
    ctx.add_xmp_metadata(&primary, xmp.as_bytes())
        .map_err(|error| HeicEncodeError::Heif(error.to_string()))?;

    let output = output.as_ref();
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let path = output
        .to_str()
        .ok_or_else(|| HeicEncodeError::InvalidPath(output.to_path_buf()))?;
    ctx.write_to_file(path)
        .map_err(|error| HeicEncodeError::Heif(error.to_string()))?;
    Ok(())
}

/// Encodes a still set's frame images (already ordered to match `set.frames`) to HEIC.
pub fn encode_still_set_heic(
    set: &DynamicStillSet,
    images: &[RgbaImage],
    output: impl AsRef<Path>,
) -> Result<(), HeicEncodeError> {
    if images.len() != set.frames.len() {
        return Err(HeicEncodeError::FrameCountMismatch {
            frames: set.frames.len(),
            images: images.len(),
        });
    }
    let (flavor, _) = apple_keys_from_still_set(set)?;
    let frames: Vec<EncodeFrame> = set
        .frames
        .iter()
        .zip(images.iter())
        .enumerate()
        .map(|(order, (frame, image))| EncodeFrame {
            index: frame
                .source_index
                .unwrap_or_else(|| u32::try_from(order).unwrap_or(u32::MAX)),
            key: frame.key,
            image: image.clone(),
        })
        .collect();
    encode_dynamic_heic(&frames, flavor, output)
}

fn open_encoder(lib: &LibHeif) -> Result<libheif_rs::Encoder<'_>, HeicEncodeError> {
    match lib.encoder_for_format(CompressionFormat::Hevc) {
        Ok(encoder) => Ok(encoder),
        Err(_) => lib
            .encoder_for_format(CompressionFormat::Av1)
            .map_err(|error| HeicEncodeError::Heif(format!("no HEVC/AV1 encoder: {error}"))),
    }
}

fn rgba_to_heif_image(source: &RgbaImage) -> Result<Image, HeicEncodeError> {
    let width = source.width();
    let height = source.height();
    if width == 0 || height == 0 {
        return Err(HeicEncodeError::EmptyImage);
    }
    // Some HEVC encoders reject odd dimensions; pad to even.
    let enc_w = width + (width % 2);
    let enc_h = height + (height % 2);

    let mut image = Image::new(enc_w, enc_h, ColorSpace::Rgb(RgbChroma::C444))
        .map_err(|error| HeicEncodeError::Heif(error.to_string()))?;
    image
        .create_plane(Channel::R, enc_w, enc_h, 8)
        .map_err(|error| HeicEncodeError::Heif(error.to_string()))?;
    image
        .create_plane(Channel::G, enc_w, enc_h, 8)
        .map_err(|error| HeicEncodeError::Heif(error.to_string()))?;
    image
        .create_plane(Channel::B, enc_w, enc_h, 8)
        .map_err(|error| HeicEncodeError::Heif(error.to_string()))?;

    let planes = image.planes_mut();
    let plane_r = planes.r.ok_or(HeicEncodeError::MissingPlanes)?;
    let plane_g = planes.g.ok_or(HeicEncodeError::MissingPlanes)?;
    let plane_b = planes.b.ok_or(HeicEncodeError::MissingPlanes)?;
    let stride = plane_r.stride;
    let data_r = plane_r.data;
    let data_g = plane_g.data;
    let data_b = plane_b.data;

    for y in 0..enc_h {
        let src_y = y.min(height - 1);
        for x in 0..enc_w {
            let src_x = x.min(width - 1);
            let pixel = source.get_pixel(src_x, src_y).0;
            let offset = y as usize * stride + x as usize;
            data_r[offset] = pixel[0];
            data_g[offset] = pixel[1];
            data_b[offset] = pixel[2];
        }
    }
    Ok(image)
}

/// HEIC encode failures.
#[derive(Debug, Error)]
pub enum HeicEncodeError {
    /// No frames were supplied.
    #[error("cannot encode an empty dynamic HEIC")]
    NoFrames,
    /// Frame / image count mismatch.
    #[error("frame count {frames} does not match image count {images}")]
    FrameCountMismatch {
        /// Domain frames.
        frames: usize,
        /// RGBA buffers.
        images: usize,
    },
    /// Empty pixel buffer.
    #[error("cannot encode a zero-sized image")]
    EmptyImage,
    /// libheif plane allocation missing.
    #[error("libheif RGB planes missing after create")]
    MissingPlanes,
    /// Output path is not valid UTF-8.
    #[error("invalid HEIC output path: {0}")]
    InvalidPath(std::path::PathBuf),
    /// Metadata build failure.
    #[error(transparent)]
    Metadata(#[from] MetadataError),
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// libheif failure.
    #[error("libheif encode error: {0}")]
    Heif(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import_dynamic_heic;
    use easel_core::{AppearanceMode, DynamicStillKey, LocalTimeOfDay};
    use image::{Rgba, RgbaImage};

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(width, height, Rgba(rgba))
    }

    #[test]
    fn roundtrips_h24_heic_through_import() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("day.heic");
        let frames = vec![
            EncodeFrame {
                index: 0,
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(6, 0).unwrap(),
                },
                image: solid(64, 48, [200, 80, 40, 255]),
            },
            EncodeFrame {
                index: 1,
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(12, 0).unwrap(),
                },
                image: solid(64, 48, [40, 120, 200, 255]),
            },
            EncodeFrame {
                index: 2,
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(18, 0).unwrap(),
                },
                image: solid(64, 48, [20, 20, 40, 255]),
            },
        ];
        encode_dynamic_heic(&frames, AppleMetadataFlavor::H24, &path).expect("encode");
        assert!(path.is_file());
        let imported = import_dynamic_heic(&path).expect("import");
        assert_eq!(imported.flavor, AppleMetadataFlavor::H24);
        assert_eq!(imported.frames.len(), 3);
        assert_eq!(imported.frames[1].key, frames[1].key);
        assert_eq!(imported.frames[0].image.width(), 64);
    }

    #[test]
    fn roundtrips_appearance_heic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("apr.heic");
        let frames = vec![
            EncodeFrame {
                index: 0,
                key: DynamicStillKey::Appearance {
                    mode: AppearanceMode::Light,
                },
                image: solid(32, 32, [240, 240, 230, 255]),
            },
            EncodeFrame {
                index: 1,
                key: DynamicStillKey::Appearance {
                    mode: AppearanceMode::Dark,
                },
                image: solid(32, 32, [20, 20, 30, 255]),
            },
        ];
        encode_dynamic_heic(&frames, AppleMetadataFlavor::Appearance, &path).expect("encode");
        let imported = import_dynamic_heic(&path).expect("import");
        assert_eq!(imported.frames.len(), 2);
        assert!(matches!(
            imported.frames[0].key,
            DynamicStillKey::Appearance {
                mode: AppearanceMode::Light
            }
        ));
    }

    #[test]
    fn remaps_sparse_frame_indices_to_encode_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sparse.heic");
        let frames = vec![
            EncodeFrame {
                index: 10,
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(18, 0).unwrap(),
                },
                image: solid(32, 24, [20, 20, 40, 255]),
            },
            EncodeFrame {
                index: 3,
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(6, 0).unwrap(),
                },
                image: solid(32, 24, [200, 80, 40, 255]),
            },
        ];
        encode_dynamic_heic(&frames, AppleMetadataFlavor::H24, &path).expect("encode");
        let imported = import_dynamic_heic(&path).expect("import");
        assert_eq!(imported.frames.len(), 2);
        assert_eq!(imported.frames[0].index, 0);
        assert_eq!(imported.frames[0].key, frames[1].key);
        assert_eq!(imported.frames[1].index, 1);
        assert_eq!(imported.frames[1].key, frames[0].key);
    }
}
