// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Raster execution and atomic wallpaper/preview output.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use easel_core::DisplayId;
use image::imageops::{self, FilterType};
use image::{Rgba, RgbaImage};
use thiserror::Error;

use crate::decode::{DecodeError, DecodedImage, decode_still};
use crate::plan::{
    CompositionSettings, OutputOperation, RENDERER_VERSION, RenderPlan, RenderPlanError,
    RenderRequest,
};

/// Completed per-display raster file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RasterOutput {
    /// Target display.
    pub display_id: DisplayId,
    /// Atomically written PNG path.
    pub path: PathBuf,
}

/// Job that decodes once and writes one PNG per display.
#[derive(Clone, Debug, PartialEq)]
pub struct RasterJob {
    /// Source image and composition request.
    pub request: RenderRequest,
    /// Directory that receives completed PNG files.
    pub output_dir: PathBuf,
}

impl RasterJob {
    /// Executes planning, decode, resampling, and atomic writes.
    pub fn execute(&self) -> Result<Vec<RasterOutput>, RasterError> {
        fs::create_dir_all(&self.output_dir)?;

        let plan = RenderPlan::for_request(&self.request)?;
        let decoded = decode_still(&self.request.source_path)?;
        let operations = plan.operations(decoded.size(), &self.request.composition)?;
        let source_token = source_cache_token(&self.request.source_path, &decoded)?;
        let arrangement_token =
            arrangement_cache_token(&self.request.displays, &self.request.composition);

        let mut outputs = Vec::with_capacity(operations.len());
        for operation in operations {
            let path = self.render_one(&decoded, &operation, &source_token, &arrangement_token)?;
            outputs.push(RasterOutput {
                display_id: operation.display_id,
                path,
            });
        }
        Ok(outputs)
    }

    fn render_one(
        &self,
        decoded: &DecodedImage,
        operation: &OutputOperation,
        source_token: &str,
        arrangement_token: &str,
    ) -> Result<PathBuf, RasterError> {
        let canvas = render_operation(&decoded.pixels, operation)?;
        let file_name = cache_file_name(
            source_token,
            operation.display_id,
            &self.request.composition,
            arrangement_token,
            operation.native_size.width,
            operation.native_size.height,
        );
        let final_path = self.output_dir.join(file_name);
        atomic_write_png(&final_path, &canvas)?;
        Ok(final_path)
    }
}

/// Renders one output operation into an RGBA canvas.
pub fn render_operation(
    source: &RgbaImage,
    operation: &OutputOperation,
) -> Result<RgbaImage, RasterError> {
    let mut canvas = RgbaImage::from_pixel(
        operation.canvas_size.width,
        operation.canvas_size.height,
        Rgba([
            operation.letterbox_color.r,
            operation.letterbox_color.g,
            operation.letterbox_color.b,
            operation.letterbox_color.a,
        ]),
    );

    let crop = operation.source_crop;
    if crop.width == 0 || crop.height == 0 {
        return Err(RasterError::EmptyCrop);
    }

    let cropped = imageops::crop_imm(
        source,
        u32::try_from(crop.x).unwrap_or(0),
        u32::try_from(crop.y).unwrap_or(0),
        crop.width,
        crop.height,
    )
    .to_image();

    let dest = operation.destination_rect;
    if dest.width == 0 || dest.height == 0 {
        // Letterbox-only output (content falls entirely outside the placed image).
        return Ok(canvas);
    }

    let resized = if cropped.width() == dest.width && cropped.height() == dest.height {
        cropped
    } else {
        imageops::resize(&cropped, dest.width, dest.height, FilterType::Lanczos3)
    };

    imageops::overlay(&mut canvas, &resized, i64::from(dest.x), i64::from(dest.y));
    Ok(canvas)
}

/// Writes PNG bytes through a temporary sibling path, then replaces the destination.
pub fn atomic_write_png(path: &Path, image: &RgbaImage) -> Result<(), RasterError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| RasterError::InvalidOutputPath(path.to_path_buf()))?;
    let part_path = path.with_file_name(format!("{file_name}.part"));
    let mut part_guard = PartFileGuard {
        path: Some(part_path.clone()),
    };

    {
        let mut file = fs::File::create(&part_path)?;
        image
            .write_to(&mut file, image::ImageFormat::Png)
            .map_err(RasterError::Image)?;
        file.flush()?;
    }

    replace_file(&part_path, path)?;
    part_guard.defuse();
    Ok(())
}

/// Removes the destination on platforms where rename cannot overwrite, then renames.
fn replace_file(from: &Path, to: &Path) -> Result<(), RasterError> {
    if to.exists() {
        fs::remove_file(to)?;
    }
    fs::rename(from, to)?;
    Ok(())
}

struct PartFileGuard {
    path: Option<PathBuf>,
}

impl PartFileGuard {
    fn defuse(&mut self) {
        self.path = None;
    }
}

impl Drop for PartFileGuard {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn cache_file_name(
    source_token: &str,
    display_id: DisplayId,
    composition: &CompositionSettings,
    arrangement_token: &str,
    width: u32,
    height: u32,
) -> String {
    let fit = format!("{:?}", composition.fit_mode).to_ascii_lowercase();
    let layout = format!("{:?}", composition.layout_mode).to_ascii_lowercase();
    let zoom = format!("{:.4}", composition.zoom);
    let focal = format!("{:.4}x{:.4}", composition.focal_x, composition.focal_y);
    let display = display_id.to_hyphenated_string().replace('-', "");
    format!(
        "v{RENDERER_VERSION}_{source_token}_{arrangement_token}_{display}_{layout}_{fit}_z{zoom}_f{focal}_{width}x{height}.png"
    )
}

/// Fingerprint of arrangement geometry that affects physical-span output.
fn arrangement_cache_token(
    displays: &[easel_core::Display],
    composition: &CompositionSettings,
) -> String {
    use std::fmt::Write as _;
    let mut material = String::new();
    let _ = write!(material, "{:?}", composition.layout_mode);
    for display in displays {
        let _ = write!(
            material,
            "|{}:{:.3},{:.3}:{:.3}x{:.3}:b{:.2},{:.2},{:.2},{:.2}:r{}:{}x{}",
            display.id.to_hyphenated_string(),
            display.physical_origin.x.0,
            display.physical_origin.y.0,
            display.physical_size.width.0,
            display.physical_size.height.0,
            display.bezel.left.0,
            display.bezel.top.0,
            display.bezel.right.0,
            display.bezel.bottom.0,
            display.rotation_degrees,
            display.native_pixels.width,
            display.native_pixels.height,
        );
    }
    format!("{:016x}", fnv1a64(material.as_bytes()))
}

fn source_cache_token(path: &Path, decoded: &DecodedImage) -> Result<String, RasterError> {
    let meta = fs::metadata(path)?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs());
    let fingerprint = fnv1a64(
        format!(
            "{}:{}:{}:{}x{}",
            path.to_string_lossy(),
            meta.len(),
            modified,
            decoded.pixels.width(),
            decoded.pixels.height()
        )
        .as_bytes(),
    );
    Ok(format!("{fingerprint:016x}"))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// Raster planning, decode, or write failure.
#[derive(Debug, Error)]
pub enum RasterError {
    /// Planning failed.
    #[error(transparent)]
    Plan(#[from] RenderPlanError),
    /// Decode failed.
    #[error(transparent)]
    Decode(#[from] DecodeError),
    /// Image crate failure while encoding or resampling.
    #[error(transparent)]
    Image(#[from] image::ImageError),
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Crop collapsed to an empty region.
    #[error("render crop is empty")]
    EmptyCrop,
    /// Output path is missing a usable file name.
    #[error("invalid output path: {0}")]
    InvalidOutputPath(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{LetterboxColor, PixelRect};
    use easel_core::{
        BezelInsets, Display, DisplayId, FitMode, LayoutMode, LogicalRect, Millimeters,
        NativePixelSize, PhysicalPoint, PhysicalSize, PhysicalSizeSource, ScaleFactor,
    };
    use image::{Rgb, RgbImage};

    fn gradient_2x1() -> RgbaImage {
        let mut image = RgbaImage::new(2, 1);
        image.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        image.put_pixel(1, 0, Rgba([255, 255, 255, 255]));
        image
    }

    #[test]
    fn cover_samples_expected_corners() {
        let source = gradient_2x1();
        let operation = OutputOperation {
            display_id: DisplayId::new(),
            native_size: NativePixelSize {
                width: 4,
                height: 4,
            },
            canvas_size: NativePixelSize {
                width: 4,
                height: 4,
            },
            source_crop: PixelRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            destination_rect: PixelRect::full(NativePixelSize {
                width: 4,
                height: 4,
            }),
            letterbox_color: LetterboxColor::default(),
        };
        let canvas = render_operation(&source, &operation).expect("render");
        assert_eq!(*canvas.get_pixel(0, 0), Rgba([0, 0, 0, 255]));
        assert_eq!(*canvas.get_pixel(3, 3), Rgba([0, 0, 0, 255]));
    }

    #[test]
    fn atomic_write_leaves_no_part_file() {
        let dir = std::env::temp_dir().join("easel-raster-atomic");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("out.png");
        let _ = fs::remove_file(&path);
        let image = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]));
        atomic_write_png(&path, &image).expect("write");
        assert!(path.is_file());
        assert!(!path.with_extension("png.part").exists());
        assert!(!dir.join("out.png.part").exists());
    }

    #[test]
    fn atomic_write_overwrites_existing_destination() {
        let dir = std::env::temp_dir().join("easel-raster-atomic-overwrite");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("out.png");
        let first = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]));
        let second = RgbaImage::from_pixel(2, 2, Rgba([9, 8, 7, 255]));
        atomic_write_png(&path, &first).expect("first write");
        atomic_write_png(&path, &second).expect("overwrite");
        assert!(path.is_file());
        assert!(!dir.join("out.png.part").exists());
        let loaded = image::open(&path).expect("open").to_rgba8();
        assert_eq!(*loaded.get_pixel(0, 0), Rgba([9, 8, 7, 255]));
    }

    #[test]
    fn raster_job_writes_per_display_outputs() {
        let dir = std::env::temp_dir().join("easel-raster-job");
        let _ = fs::create_dir_all(&dir);
        let source_path = dir.join("source.png");
        RgbImage::from_pixel(8, 8, Rgb([40, 80, 120]))
            .save(&source_path)
            .expect("save source");

        let display = Display {
            id: DisplayId::from_u128(1),
            connector_name: Some("eDP-1".into()),
            manufacturer: None,
            model: None,
            serial: None,
            logical_rect: LogicalRect {
                x: 0,
                y: 0,
                width: 16,
                height: 16,
            },
            native_pixels: NativePixelSize {
                width: 16,
                height: 16,
            },
            scale_factor: ScaleFactor::default(),
            physical_size: PhysicalSize {
                width: Millimeters(300.0),
                height: Millimeters(200.0),
            },
            physical_size_source: PhysicalSizeSource::Detected,
            physical_origin: PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
            bezel: BezelInsets::default(),
            rotation_degrees: 0,
        };

        let job = RasterJob {
            request: RenderRequest {
                source_path,
                displays: vec![display],
                composition: CompositionSettings {
                    fit_mode: FitMode::Cover,
                    layout_mode: LayoutMode::Digital,
                    zoom: 1.0,
                    focal_x: 0.5,
                    focal_y: 0.5,
                },
                purpose: crate::plan::RenderPurpose::StaticWallpaper,
            },
            output_dir: dir.join("out"),
        };

        let outputs = job.execute().expect("execute");
        assert_eq!(outputs.len(), 1);
        assert!(outputs[0].path.is_file());
        assert!(!outputs[0].path.with_extension("png.part").exists());
    }
}
