// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Resolution and aspect suitability for selected display groups.

use crate::{Display, MediaDimensions};

/// Pixel budget derived from a selected display group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelBudget {
    /// Widest native width among selected displays.
    pub max_display_width: u32,
    /// Tallest native height among selected displays.
    pub max_display_height: u32,
    /// Sum of native pixel areas across selected displays.
    pub total_area: u64,
    /// Approximate span width when displays are treated as a horizontal row.
    pub span_width: u32,
    /// Approximate span height when displays are treated as a vertical stack.
    pub span_height: u32,
}

impl PixelBudget {
    /// Computes a budget from the provided displays.
    #[must_use]
    pub fn from_displays(displays: &[Display]) -> Self {
        if displays.is_empty() {
            return Self {
                max_display_width: 0,
                max_display_height: 0,
                total_area: 0,
                span_width: 0,
                span_height: 0,
            };
        }

        let max_display_width = displays
            .iter()
            .map(|display| display.native_pixels.width)
            .max()
            .unwrap_or(0);
        let max_display_height = displays
            .iter()
            .map(|display| display.native_pixels.height)
            .max()
            .unwrap_or(0);
        let total_area = displays.iter().fold(0_u64, |acc, display| {
            acc.saturating_add(
                u64::from(display.native_pixels.width)
                    .saturating_mul(u64::from(display.native_pixels.height)),
            )
        });
        let span_width = displays.iter().fold(0_u32, |acc, display| {
            acc.saturating_add(display.native_pixels.width)
        });
        let span_height = displays.iter().fold(0_u32, |acc, display| {
            acc.saturating_add(display.native_pixels.height)
        });

        Self {
            max_display_width,
            max_display_height,
            total_area,
            span_width,
            span_height,
        }
    }
}

/// Suitability of one asset for a display-group pixel budget.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuitabilityAssessment {
    /// Score from 0 (unsuitable) to 100 (excellent).
    pub score: u8,
    /// Whether the asset meets the minimum no-upscale bar for every selected display.
    pub meets_minimum: bool,
    /// Short machine-readable reasons contributing to the score.
    pub reasons: Vec<&'static str>,
}

/// Scores how well `asset` covers `budget` without heavy upscaling.
#[must_use]
pub fn assess_suitability(asset: MediaDimensions, budget: PixelBudget) -> SuitabilityAssessment {
    if budget.max_display_width == 0 || budget.max_display_height == 0 {
        return SuitabilityAssessment {
            score: 0,
            meets_minimum: false,
            reasons: vec!["empty_display_group"],
        };
    }

    let mut reasons = Vec::new();
    let meets_minimum =
        asset.width >= budget.max_display_width && asset.height >= budget.max_display_height;
    if meets_minimum {
        reasons.push("covers_largest_display");
    } else {
        reasons.push("below_largest_display");
    }

    let asset_area = u64::from(asset.width).saturating_mul(u64::from(asset.height));
    let mut score = 0_u8;
    if meets_minimum {
        score = score.saturating_add(40);
    }

    if budget.total_area > 0 && asset_area >= budget.total_area {
        score = score.saturating_add(40);
        reasons.push("covers_total_area");
    } else if budget.total_area > 0
        && asset_area.saturating_mul(10) >= budget.total_area.saturating_mul(7)
    {
        score = score.saturating_add(25);
        reasons.push("near_total_area");
    } else if budget.total_area > 0
        && asset_area.saturating_mul(10) >= budget.total_area.saturating_mul(4)
    {
        score = score.saturating_add(10);
        reasons.push("partial_total_area");
    } else {
        reasons.push("insufficient_total_area");
    }

    let asset_aspect_milli = aspect_milli(asset.width, asset.height);
    let span_aspect_milli =
        aspect_milli(budget.span_width.max(1), budget.max_display_height.max(1));
    let aspect_delta = asset_aspect_milli.abs_diff(span_aspect_milli);
    if aspect_delta <= 150 {
        score = score.saturating_add(20);
        reasons.push("aspect_close_to_span");
    } else if aspect_delta <= 400 {
        score = score.saturating_add(10);
        reasons.push("aspect_acceptable");
    } else {
        reasons.push("aspect_mismatch");
    }

    SuitabilityAssessment {
        score: score.min(100),
        meets_minimum,
        reasons,
    }
}

/// Width/height aspect ratio in thousandths to avoid floating-point scoring.
fn aspect_milli(width: u32, height: u32) -> u32 {
    u32::try_from(u64::from(width.max(1)).saturating_mul(1000) / u64::from(height.max(1)))
        .unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BezelInsets, Display, DisplayId, LogicalRect, Millimeters, NativePixelSize, PhysicalPoint,
        PhysicalSize, PhysicalSizeSource, ScaleFactor,
    };

    fn display(width: u32, height: u32) -> Display {
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
            scale_factor: ScaleFactor::new(1, 1).expect("scale"),
            physical_size: PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(300.0),
            },
            physical_size_source: PhysicalSizeSource::Detected,
            physical_origin: PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
            rotation_degrees: 0,
            bezel: BezelInsets::default(),
        }
    }

    #[test]
    fn rejects_assets_smaller_than_largest_display() {
        let budget = PixelBudget::from_displays(&[display(3840, 2160)]);
        let assessment = assess_suitability(
            MediaDimensions {
                width: 1920,
                height: 1080,
            },
            budget,
        );
        assert!(!assessment.meets_minimum);
        assert!(assessment.score < 50);
    }

    #[test]
    fn rewards_span_capable_assets() {
        let budget = PixelBudget::from_displays(&[display(1920, 1080), display(1920, 1080)]);
        let assessment = assess_suitability(
            MediaDimensions {
                width: 3840,
                height: 1080,
            },
            budget,
        );
        assert!(assessment.meets_minimum);
        assert!(assessment.score >= 80);
    }
}
