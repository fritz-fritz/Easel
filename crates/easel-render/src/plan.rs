// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic render planning before media bytes are sampled.

use std::path::PathBuf;

use easel_core::{
    Display, DisplayId, DisplayValidationError, FitMode, LayoutMode, NativePixelSize,
    PhysicalLayoutError, Profile, ProfileValidationError, content_bounds, content_rect,
};
use thiserror::Error;

use crate::fit::plan_fit;

/// Version token included in cache keys when raster semantics change.
pub const RENDERER_VERSION: &str = "2";

/// Why a deterministic raster output is being produced.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RenderPurpose {
    /// A completed still image for an operating-system wallpaper API.
    #[default]
    StaticWallpaper,
    /// The safe fallback shown before or instead of live playback.
    LivePosterFrame,
    /// A frame consumed by a live compositor when native video transforms are unavailable.
    LiveCompositorFrame,
}

/// Axis-aligned rectangle in integer pixel space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelRect {
    /// Left edge; may be negative for destination placements that are later clipped.
    pub x: i32,
    /// Top edge; may be negative for destination placements that are later clipped.
    pub y: i32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl PixelRect {
    /// Returns a rectangle covering an entire size origin.
    #[must_use]
    pub fn full(size: NativePixelSize) -> Self {
        Self {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        }
    }

    /// Clamps this rectangle so it stays inside `bounds` with at least a 1×1 area when possible.
    pub fn clamp_to(&mut self, bounds: NativePixelSize) {
        if bounds.width == 0 || bounds.height == 0 {
            self.width = 0;
            self.height = 0;
            return;
        }

        let mut left = i64::from(self.x).max(0);
        let mut top = i64::from(self.y).max(0);
        let mut right = left + i64::from(self.width);
        let mut bottom = top + i64::from(self.height);

        left = left.min(i64::from(bounds.width.saturating_sub(1)));
        top = top.min(i64::from(bounds.height.saturating_sub(1)));
        right = right.clamp(left + 1, i64::from(bounds.width));
        bottom = bottom.clamp(top + 1, i64::from(bounds.height));

        self.x = i32::try_from(left).unwrap_or(i32::MAX);
        self.y = i32::try_from(top).unwrap_or(i32::MAX);
        self.width = u32::try_from(right - left).unwrap_or(1);
        self.height = u32::try_from(bottom - top).unwrap_or(1);
    }
}

/// Neutral fill behind letterboxed contain/native placements.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LetterboxColor {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel.
    pub a: u8,
}

impl Default for LetterboxColor {
    fn default() -> Self {
        Self {
            r: 24,
            g: 24,
            b: 28,
            a: 255,
        }
    }
}

/// Profile composition fields consumed by the planner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompositionSettings {
    /// Scaling behavior.
    pub fit_mode: FitMode,
    /// Physical span versus per-display digital fitting.
    pub layout_mode: LayoutMode,
    /// Zoom multiplier applied during planning.
    ///
    /// Non-finite values become `1.0`. Values below one are clamped to one when planning.
    /// Persistent [`Profile`] values still reject zoom below one via [`Profile::validate`].
    pub zoom: f64,
    /// Horizontal focal point from zero through one.
    pub focal_x: f64,
    /// Vertical focal point from zero through one.
    pub focal_y: f64,
}

impl CompositionSettings {
    /// Extracts composition fields from a validated profile.
    #[must_use]
    pub fn from_profile(profile: &Profile) -> Self {
        Self {
            fit_mode: profile.fit_mode,
            layout_mode: profile.layout_mode,
            zoom: profile.zoom,
            focal_x: profile.focal_x,
            focal_y: profile.focal_y,
        }
        .normalized()
    }

    /// Returns settings with finite zoom ≥ 1 and focal points clamped to `0..=1`.
    #[must_use]
    pub fn normalized(self) -> Self {
        Self {
            fit_mode: self.fit_mode,
            layout_mode: self.layout_mode,
            zoom: if self.zoom.is_finite() {
                self.zoom.max(1.0)
            } else {
                1.0
            },
            focal_x: self.focal_x.clamp(0.0, 1.0),
            focal_y: self.focal_y.clamp(0.0, 1.0),
        }
    }
}

/// Request describing one static render of a local still image.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderRequest {
    /// Absolute or relative path to the local source still.
    pub source_path: PathBuf,
    /// Displays that receive output.
    pub displays: Vec<Display>,
    /// Cover/contain/zoom/focal settings.
    pub composition: CompositionSettings,
    /// Consumer of the completed raster output.
    pub purpose: RenderPurpose,
}

/// Output requested for one physical display.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputPlan {
    /// Target display.
    pub display_id: DisplayId,
    /// Exact output dimensions.
    pub native_size: NativePixelSize,
}

/// Per-display crop and placement once source dimensions are known.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputOperation {
    /// Target display.
    pub display_id: DisplayId,
    /// Exact output dimensions.
    pub native_size: NativePixelSize,
    /// Canvas size; identical to [`Self::native_size`] for Stage 1.
    pub canvas_size: NativePixelSize,
    /// Region sampled from the oriented source image.
    pub source_crop: PixelRect,
    /// Placement of the resampled crop on the output canvas.
    pub destination_rect: PixelRect,
    /// Fill color behind uncovered canvas pixels.
    pub letterbox_color: LetterboxColor,
}

/// Deterministic plan produced before decoding media bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderPlan {
    /// Consumer of the completed raster output.
    pub purpose: RenderPurpose,
    /// One native output per participating display.
    pub outputs: Vec<OutputPlan>,
    /// Displays retained so physical-span operations can consult geometry.
    pub displays: Vec<Display>,
}

impl RenderPlan {
    /// Builds the initial per-output plan while validating display invariants.
    pub fn for_displays(displays: &[Display]) -> Result<Self, RenderPlanError> {
        Self::for_purpose(displays, RenderPurpose::StaticWallpaper)
    }

    /// Builds a per-output plan for a specific static or live consumer.
    pub fn for_purpose(
        displays: &[Display],
        purpose: RenderPurpose,
    ) -> Result<Self, RenderPlanError> {
        if displays.is_empty() {
            return Err(RenderPlanError::NoDisplays);
        }

        let outputs = displays
            .iter()
            .map(|display| {
                display.validate()?;
                Ok(OutputPlan {
                    display_id: display.id,
                    native_size: display.native_pixels,
                })
            })
            .collect::<Result<Vec<_>, DisplayValidationError>>()?;

        Ok(Self {
            purpose,
            outputs,
            displays: displays.to_vec(),
        })
    }

    /// Validates a full request and returns display output slots.
    pub fn for_request(request: &RenderRequest) -> Result<Self, RenderPlanError> {
        if request.source_path.as_os_str().is_empty() {
            return Err(RenderPlanError::EmptySourcePath);
        }
        validate_composition(&request.composition)?;
        Self::for_purpose(&request.displays, request.purpose)
    }

    /// Materializes crop/placement ops once oriented source dimensions are known.
    pub fn operations(
        &self,
        source_size: NativePixelSize,
        composition: &CompositionSettings,
    ) -> Result<Vec<OutputOperation>, RenderPlanError> {
        if source_size.width == 0 || source_size.height == 0 {
            return Err(RenderPlanError::EmptySourceSize);
        }
        let composition = composition.normalized();
        validate_composition(&composition)?;

        match composition.layout_mode {
            LayoutMode::Digital => Ok(self.digital_operations(source_size, &composition)),
            LayoutMode::PhysicalSpan => self.physical_operations(source_size, &composition),
        }
    }

    fn digital_operations(
        &self,
        source_size: NativePixelSize,
        composition: &CompositionSettings,
    ) -> Vec<OutputOperation> {
        self.outputs
            .iter()
            .map(|output| {
                let (source_crop, destination_rect) =
                    plan_fit(source_size, output.native_size, composition);
                OutputOperation {
                    display_id: output.display_id,
                    native_size: output.native_size,
                    canvas_size: output.native_size,
                    source_crop,
                    destination_rect,
                    letterbox_color: LetterboxColor::default(),
                }
            })
            .collect()
    }

    fn physical_operations(
        &self,
        source_size: NativePixelSize,
        composition: &CompositionSettings,
    ) -> Result<Vec<OutputOperation>, RenderPlanError> {
        let span = content_bounds(&self.displays)?;
        let span_w = span.width.0;
        let span_h = span.height.0;
        if span_w <= 0.0 || span_h <= 0.0 {
            return Err(RenderPlanError::Physical(
                PhysicalLayoutError::InvalidPhysicalSize,
            ));
        }

        let (src_x, src_y, src_w, src_h, image_x, image_y, image_w, image_h) =
            place_source_on_span(source_size, span_w, span_h, composition);

        let mut operations = Vec::with_capacity(self.displays.len());
        for display in &self.displays {
            let content = content_rect(display)?;
            let native = display.native_pixels;
            let (source_crop, destination_rect) = map_content_to_operation(
                source_size,
                native,
                content.x.0,
                content.y.0,
                content.width.0,
                content.height.0,
                src_x,
                src_y,
                src_w,
                src_h,
                image_x + span.x.0,
                image_y + span.y.0,
                image_w,
                image_h,
                composition.fit_mode,
            );
            operations.push(OutputOperation {
                display_id: display.id,
                native_size: native,
                canvas_size: native,
                source_crop,
                destination_rect,
                letterbox_color: LetterboxColor::default(),
            });
        }
        Ok(operations)
    }
}

/// Places the source onto a span-sized canvas in millimeter space.
///
/// Returns `(src_x, src_y, src_w, src_h, image_x, image_y, image_w, image_h)` where the source
/// crop describes the portion of the image that covers the span for Cover-like modes, and the
/// image rect describes where the full source sits relative to the span origin for Contain.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines
)]
fn place_source_on_span(
    source: NativePixelSize,
    span_w: f64,
    span_h: f64,
    composition: &CompositionSettings,
) -> (f64, f64, f64, f64, f64, f64, f64, f64) {
    let zoom = composition.zoom.max(1.0);
    let src_w = f64::from(source.width);
    let src_h = f64::from(source.height);

    match composition.fit_mode {
        FitMode::Stretch => (0.0, 0.0, src_w, src_h, 0.0, 0.0, span_w, span_h),
        FitMode::Native => {
            // Map one source pixel to one millimeter of span for a deterministic physical native
            // placement, then focal-bias within the span.
            let image_w = src_w;
            let image_h = src_h;
            let max_x = (image_w - span_w).max(0.0);
            let max_y = (image_h - span_h).max(0.0);
            let image_x = -(composition.focal_x * max_x);
            let image_y = -(composition.focal_y * max_y);
            let crop_w = span_w.min(src_w);
            let crop_h = span_h.min(src_h);
            let src_x = composition.focal_x * (src_w - crop_w).max(0.0);
            let src_y = composition.focal_y * (src_h - crop_h).max(0.0);
            (
                src_x, src_y, crop_w, crop_h, image_x, image_y, image_w, image_h,
            )
        }
        FitMode::Cover => {
            let scale = (span_w / src_w).max(span_h / src_h) * zoom;
            let viewed_w = (span_w / scale).min(src_w);
            let viewed_h = (span_h / scale).min(src_h);
            let src_x = composition.focal_x * (src_w - viewed_w).max(0.0);
            let src_y = composition.focal_y * (src_h - viewed_h).max(0.0);
            (src_x, src_y, viewed_w, viewed_h, 0.0, 0.0, span_w, span_h)
        }
        FitMode::Contain => {
            let scale = (span_w / src_w).min(span_h / src_h) * zoom;
            let image_w = src_w * scale;
            let image_h = src_h * scale;
            if image_w <= span_w && image_h <= span_h {
                let image_x = (span_w - image_w) / 2.0;
                let image_y = (span_h - image_h) / 2.0;
                (0.0, 0.0, src_w, src_h, image_x, image_y, image_w, image_h)
            } else {
                let viewed_w = (span_w / scale).min(src_w);
                let viewed_h = (span_h / scale).min(src_h);
                let src_x = composition.focal_x * (src_w - viewed_w).max(0.0);
                let src_y = composition.focal_y * (src_h - viewed_h).max(0.0);
                (src_x, src_y, viewed_w, viewed_h, 0.0, 0.0, span_w, span_h)
            }
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_arguments
)]
fn map_content_to_operation(
    source: NativePixelSize,
    native: NativePixelSize,
    content_x: f64,
    content_y: f64,
    content_w: f64,
    content_h: f64,
    src_x: f64,
    src_y: f64,
    src_w: f64,
    src_h: f64,
    image_x: f64,
    image_y: f64,
    image_w: f64,
    image_h: f64,
    fit_mode: FitMode,
) -> (PixelRect, PixelRect) {
    match fit_mode {
        FitMode::Cover | FitMode::Stretch | FitMode::Native => {
            // Span maps linearly onto the source crop window.
            let span_x = image_x;
            let span_y = image_y;
            // For Cover/Stretch/Native-cover path, image rect equals the span at (span origin).
            // content relative to the image/span mapping:
            let rel_x = ((content_x - span_x) / image_w).clamp(0.0, 1.0);
            let rel_y = ((content_y - span_y) / image_h).clamp(0.0, 1.0);
            let rel_w = (content_w / image_w).clamp(0.0, 1.0);
            let rel_h = (content_h / image_h).clamp(0.0, 1.0);
            let crop = PixelRect {
                x: (src_x + rel_x * src_w).floor() as i32,
                y: (src_y + rel_y * src_h).floor() as i32,
                width: (rel_w * src_w).round().clamp(1.0, f64::from(source.width)) as u32,
                height: (rel_h * src_h).round().clamp(1.0, f64::from(source.height)) as u32,
            };
            let mut crop = crop;
            crop.clamp_to(source);
            (crop, PixelRect::full(native))
        }
        FitMode::Contain => {
            // Intersect content rect with the placed image in absolute mm space.
            let img_left = image_x;
            let img_top = image_y;
            let img_right = image_x + image_w;
            let img_bottom = image_y + image_h;
            let left = content_x.max(img_left);
            let top = content_y.max(img_top);
            let right = (content_x + content_w).min(img_right);
            let bottom = (content_y + content_h).min(img_bottom);

            if right <= left || bottom <= top {
                return (
                    PixelRect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    PixelRect {
                        x: 0,
                        y: 0,
                        width: 0,
                        height: 0,
                    },
                );
            }

            let src_left = src_x + (left - img_left) / image_w * src_w;
            let src_top = src_y + (top - img_top) / image_h * src_h;
            let src_right = src_x + (right - img_left) / image_w * src_w;
            let src_bottom = src_y + (bottom - img_top) / image_h * src_h;

            let mut crop = PixelRect {
                x: src_left.floor() as i32,
                y: src_top.floor() as i32,
                width: (src_right - src_left)
                    .round()
                    .clamp(1.0, f64::from(source.width)) as u32,
                height: (src_bottom - src_top)
                    .round()
                    .clamp(1.0, f64::from(source.height)) as u32,
            };
            crop.clamp_to(source);

            let dest_x = ((left - content_x) / content_w * f64::from(native.width)).round() as i32;
            let dest_y = ((top - content_y) / content_h * f64::from(native.height)).round() as i32;
            let dest_w = (((right - left) / content_w * f64::from(native.width)).round() as u32)
                .clamp(1, native.width);
            let dest_h = (((bottom - top) / content_h * f64::from(native.height)).round() as u32)
                .clamp(1, native.height);

            (
                crop,
                PixelRect {
                    x: dest_x,
                    y: dest_y,
                    width: dest_w,
                    height: dest_h,
                },
            )
        }
    }
}

fn validate_composition(composition: &CompositionSettings) -> Result<(), RenderPlanError> {
    // Planning clamps zoom and focals via [`CompositionSettings::normalized`]. Reject only
    // values that cannot be repaired (non-finite zoom).
    if !composition.zoom.is_finite() {
        return Err(RenderPlanError::InvalidComposition(
            ProfileValidationError::InvalidZoom,
        ));
    }
    Ok(())
}

/// Render planning failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RenderPlanError {
    /// At least one display is required.
    #[error("render plan requires at least one display")]
    NoDisplays,
    /// Source path must be non-empty.
    #[error("render request requires a source path")]
    EmptySourcePath,
    /// Decoded source dimensions must be non-zero.
    #[error("source dimensions must be non-zero")]
    EmptySourceSize,
    /// A display record failed validation.
    #[error(transparent)]
    InvalidDisplay(#[from] DisplayValidationError),
    /// Composition settings failed validation.
    #[error(transparent)]
    InvalidComposition(#[from] ProfileValidationError),
    /// Physical layout geometry failed validation.
    #[error(transparent)]
    Physical(#[from] PhysicalLayoutError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::{
        BezelInsets, LogicalRect, Millimeters, PhysicalPoint, PhysicalSize, PhysicalSizeSource,
        ScaleFactor,
    };

    fn sample_display(width: u32, height: u32) -> Display {
        Display {
            id: DisplayId::new(),
            connector_name: Some("DP-1".into()),
            manufacturer: None,
            model: None,
            serial: None,
            logical_rect: LogicalRect {
                x: 0,
                y: 0,
                width,
                height,
            },
            native_pixels: NativePixelSize { width, height },
            scale_factor: ScaleFactor::default(),
            physical_size: PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(300.0),
            },
            physical_size_source: PhysicalSizeSource::Detected,
            physical_origin: PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
            bezel: BezelInsets::default(),
            rotation_degrees: 0,
        }
    }

    #[test]
    fn empty_layout_is_rejected() {
        assert_eq!(
            RenderPlan::for_displays(&[]),
            Err(RenderPlanError::NoDisplays)
        );
    }

    #[test]
    fn operations_cover_each_display() {
        let displays = vec![sample_display(100, 100), sample_display(200, 100)];
        let plan = RenderPlan::for_displays(&displays).expect("plan");
        let ops = plan
            .operations(
                NativePixelSize {
                    width: 400,
                    height: 200,
                },
                &CompositionSettings {
                    fit_mode: FitMode::Cover,
                    layout_mode: LayoutMode::Digital,
                    zoom: 1.0,
                    focal_x: 0.5,
                    focal_y: 0.5,
                },
            )
            .expect("ops");
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].canvas_size.width, 100);
        assert_eq!(ops[1].canvas_size.width, 200);
    }

    #[test]
    fn operations_clamp_sub_one_zoom() {
        let displays = vec![sample_display(100, 100)];
        let plan = RenderPlan::for_displays(&displays).expect("plan");
        let ops = plan
            .operations(
                NativePixelSize {
                    width: 200,
                    height: 200,
                },
                &CompositionSettings {
                    fit_mode: FitMode::Cover,
                    layout_mode: LayoutMode::Digital,
                    zoom: 0.25,
                    focal_x: 1.5,
                    focal_y: -0.5,
                },
            )
            .expect("ops");
        assert_eq!(ops.len(), 1);
        // Clamped zoom of 1.0 with Cover on equal aspects uses the full source.
        assert_eq!(ops[0].source_crop.width, 200);
        assert_eq!(ops[0].source_crop.height, 200);
    }

    #[test]
    fn physical_span_splits_source_across_row() {
        let mut left = sample_display(100, 100);
        left.id = DisplayId::from_u128(1);
        left.physical_size = PhysicalSize {
            width: Millimeters(400.0),
            height: Millimeters(300.0),
        };
        let mut right = sample_display(100, 100);
        right.id = DisplayId::from_u128(2);
        right.physical_origin = PhysicalPoint {
            x: Millimeters(400.0),
            y: Millimeters(0.0),
        };
        right.physical_size = PhysicalSize {
            width: Millimeters(400.0),
            height: Millimeters(300.0),
        };

        let plan = RenderPlan::for_displays(&[left, right]).expect("plan");
        let ops = plan
            .operations(
                NativePixelSize {
                    width: 200,
                    height: 100,
                },
                &CompositionSettings {
                    fit_mode: FitMode::Cover,
                    layout_mode: LayoutMode::PhysicalSpan,
                    zoom: 1.0,
                    focal_x: 0.5,
                    focal_y: 0.5,
                },
            )
            .expect("ops");
        assert_eq!(ops.len(), 2);
        assert!(ops[0].source_crop.x < ops[1].source_crop.x);
        assert_eq!(ops[0].source_crop.width, ops[1].source_crop.width);
    }
}
