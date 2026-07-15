//! Deterministic render planning before media bytes are sampled.

use std::path::PathBuf;

use thiserror::Error;
use wallspan_core::{
    Display, DisplayId, DisplayValidationError, FitMode, NativePixelSize, Profile,
    ProfileValidationError,
};

use crate::fit::plan_fit;

/// Version token included in cache keys when raster semantics change.
pub const RENDERER_VERSION: &str = "1";

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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderPlan {
    /// Consumer of the completed raster output.
    pub purpose: RenderPurpose,
    /// One native output per participating display.
    pub outputs: Vec<OutputPlan>,
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

        Ok(Self { purpose, outputs })
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

        Ok(self
            .outputs
            .iter()
            .map(|output| {
                let (source_crop, destination_rect) =
                    plan_fit(source_size, output.native_size, &composition);
                OutputOperation {
                    display_id: output.display_id,
                    native_size: output.native_size,
                    canvas_size: output.native_size,
                    source_crop,
                    destination_rect,
                    letterbox_color: LetterboxColor::default(),
                }
            })
            .collect())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use wallspan_core::{LogicalRect, Millimeters, PhysicalPoint, PhysicalSize, ScaleFactor};

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
            physical_origin: PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
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
}
