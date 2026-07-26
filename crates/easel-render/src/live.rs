// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Multi-display live crop planning for a shared media clock.
//!
//! Live surfaces reuse the same crop/placement math as still posters
//! ([`RenderPurpose::LiveCompositorFrame`]) so a Plasma (or other) host can
//! apply one decoded frame with synchronized per-display transforms.

use easel_core::{Display, DisplayId, LogicalRect, NativePixelSize};

use crate::plan::{
    CompositionSettings, LetterboxColor, PixelRect, RenderPlan, RenderPlanError, RenderPurpose,
};

/// Normalized source rectangle in `0..=1` UV space for GPU / QML consumers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormalizedRect {
    /// Left edge of the source sample window.
    pub x: f64,
    /// Top edge of the source sample window.
    pub y: f64,
    /// Width of the source sample window.
    pub width: f64,
    /// Height of the source sample window.
    pub height: f64,
}

/// Per-display crop and placement derived from the shared composition plan.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveDisplayCrop {
    /// Target display identity.
    pub display_id: DisplayId,
    /// Native output size for the surface.
    pub native_size: NativePixelSize,
    /// Logical compositor rectangle used to match a host output.
    pub logical_rect: LogicalRect,
    /// Integer crop in oriented source pixels.
    pub source_crop: PixelRect,
    /// Placement of the resampled crop on the output canvas.
    pub destination_rect: PixelRect,
    /// Normalized UV window matching [`Self::source_crop`].
    pub source_uv: NormalizedRect,
    /// Fill behind uncovered canvas pixels (Contain / letterbox).
    pub letterbox_color: LetterboxColor,
}

/// Plans per-display live crops for one source size and composition.
///
/// Every crop shares the same source media timeline; hosts must not open an
/// independent player per display.
pub fn plan_live_crops(
    source_size: NativePixelSize,
    displays: &[Display],
    composition: &CompositionSettings,
) -> Result<Vec<LiveDisplayCrop>, RenderPlanError> {
    let plan = RenderPlan::for_purpose(displays, RenderPurpose::LiveCompositorFrame)?;
    let operations = plan.operations(source_size, composition)?;
    let mut crops = Vec::with_capacity(operations.len());
    for operation in operations {
        let display = displays
            .iter()
            .find(|candidate| candidate.id == operation.display_id)
            .ok_or(RenderPlanError::NoDisplays)?;
        crops.push(LiveDisplayCrop {
            display_id: operation.display_id,
            native_size: operation.native_size,
            logical_rect: display.logical_rect,
            source_crop: operation.source_crop,
            destination_rect: operation.destination_rect,
            source_uv: normalized_source_uv(operation.source_crop, source_size),
            letterbox_color: operation.letterbox_color,
        });
    }
    Ok(crops)
}

fn normalized_source_uv(crop: PixelRect, source: NativePixelSize) -> NormalizedRect {
    let width = f64::from(source.width.max(1));
    let height = f64::from(source.height.max(1));
    NormalizedRect {
        x: f64::from(crop.x.max(0)) / width,
        y: f64::from(crop.y.max(0)) / height,
        width: f64::from(crop.width) / width,
        height: f64::from(crop.height) / height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::{
        BezelInsets, DisplayId, FitMode, LayoutMode, LogicalRect, Millimeters, PhysicalPoint,
        PhysicalSize, PhysicalSizeSource, ScaleFactor, two_equal_row,
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
    fn live_crops_match_still_operations() {
        let displays = two_equal_row().displays;
        let source = NativePixelSize {
            width: 400,
            height: 200,
        };
        let composition = CompositionSettings {
            fit_mode: FitMode::Cover,
            layout_mode: LayoutMode::PhysicalSpan,
            zoom: 1.0,
            focal_x: 0.5,
            focal_y: 0.5,
        };
        let crops = plan_live_crops(source, &displays, &composition).expect("crops");
        let ops = RenderPlan::for_purpose(&displays, RenderPurpose::LivePosterFrame)
            .expect("plan")
            .operations(source, &composition)
            .expect("ops");
        assert_eq!(crops.len(), ops.len());
        for (crop, op) in crops.iter().zip(ops.iter()) {
            assert_eq!(crop.display_id, op.display_id);
            assert_eq!(crop.source_crop, op.source_crop);
            assert_eq!(crop.destination_rect, op.destination_rect);
            assert_eq!(crop.native_size, op.native_size);
        }
        // Spanned row: left crop starts left of right crop.
        assert!(crops[0].source_crop.x < crops[1].source_crop.x);
    }

    #[test]
    fn digital_mode_produces_independent_uv_windows() {
        let displays = vec![sample_display(100, 100), sample_display(200, 100)];
        let source = NativePixelSize {
            width: 400,
            height: 200,
        };
        let composition = CompositionSettings {
            fit_mode: FitMode::Cover,
            layout_mode: LayoutMode::Digital,
            zoom: 1.0,
            focal_x: 0.5,
            focal_y: 0.5,
        };
        let crops = plan_live_crops(source, &displays, &composition).expect("crops");
        assert_eq!(crops.len(), 2);
        for crop in &crops {
            assert!(crop.source_uv.width > 0.0);
            assert!(crop.source_uv.height > 0.0);
            assert!(crop.source_uv.x >= 0.0);
            assert!(crop.source_uv.y >= 0.0);
            assert!(crop.source_uv.x + crop.source_uv.width <= 1.0 + 1e-9);
            assert!(crop.source_uv.y + crop.source_uv.height <= 1.0 + 1e-9);
        }
    }
}
