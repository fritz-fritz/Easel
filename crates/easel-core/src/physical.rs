// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Physical layout space helpers: PPI, bezels, and millimeter rectangles.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    Display, DisplayValidationError, Millimeters, NativePixelSize, PhysicalPoint, PhysicalSize,
};

/// Millimeters per inch, used when converting PPI ↔ panel size.
pub const MM_PER_INCH: f64 = 25.4;

/// Axis-aligned rectangle in physical layout space (millimeters).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalRect {
    /// Left edge.
    pub x: Millimeters,
    /// Top edge.
    pub y: Millimeters,
    /// Width.
    pub width: Millimeters,
    /// Height.
    pub height: Millimeters,
}

impl PhysicalRect {
    /// Returns the right edge coordinate.
    #[must_use]
    pub fn right(self) -> Millimeters {
        Millimeters(self.x.0 + self.width.0)
    }

    /// Returns the bottom edge coordinate.
    #[must_use]
    pub fn bottom(self) -> Millimeters {
        Millimeters(self.y.0 + self.height.0)
    }

    /// Returns the axis-aligned union of `self` and `other`.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let x = self.x.0.min(other.x.0);
        let y = self.y.0.min(other.y.0);
        let right = self.right().0.max(other.right().0);
        let bottom = self.bottom().0.max(other.bottom().0);
        Self {
            x: Millimeters(x),
            y: Millimeters(y),
            width: Millimeters((right - x).max(0.0)),
            height: Millimeters((bottom - y).max(0.0)),
        }
    }

    /// Returns true when width and height are positive and finite.
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.width.0.is_finite()
            && self.height.0.is_finite()
            && self.width.0 > 0.0
            && self.height.0 > 0.0
            && self.x.0.is_finite()
            && self.y.0.is_finite()
    }
}

/// Bezel thickness on each edge of a panel, measured outward from the active area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BezelInsets {
    /// Left bezel width.
    pub left: Millimeters,
    /// Top bezel height.
    pub top: Millimeters,
    /// Right bezel width.
    pub right: Millimeters,
    /// Bottom bezel height.
    pub bottom: Millimeters,
}

impl BezelInsets {
    /// Uniform bezel on every edge.
    #[must_use]
    pub const fn uniform(mm: f64) -> Self {
        Self {
            left: Millimeters(mm),
            top: Millimeters(mm),
            right: Millimeters(mm),
            bottom: Millimeters(mm),
        }
    }

    /// Validates finite, non-negative insets.
    pub fn validate(self) -> Result<(), PhysicalLayoutError> {
        for edge in [self.left.0, self.top.0, self.right.0, self.bottom.0] {
            if !edge.is_finite() || edge < 0.0 {
                return Err(PhysicalLayoutError::InvalidBezel);
            }
        }
        Ok(())
    }

    /// Horizontal inset total.
    #[must_use]
    pub fn horizontal(self) -> Millimeters {
        Millimeters(self.left.0 + self.right.0)
    }

    /// Vertical inset total.
    #[must_use]
    pub fn vertical(self) -> Millimeters {
        Millimeters(self.top.0 + self.bottom.0)
    }
}

/// Whether [`Display::physical_size`] came from the platform or a user override.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalSizeSource {
    /// Size reported by EDID/platform probing.
    #[default]
    Detected,
    /// Size entered or confirmed by the user; rematching must preserve it.
    UserOverride,
}

/// Pixels-per-inch along each panel axis after accounting for rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ppi {
    /// Horizontal PPI (along the physical width axis).
    pub x: f64,
    /// Vertical PPI (along the physical height axis).
    pub y: f64,
}

impl Ppi {
    /// Derives PPI from native framebuffer size and physical panel size.
    ///
    /// For 90°/270° rotations the native width maps to physical height.
    pub fn from_display(
        native: NativePixelSize,
        physical: PhysicalSize,
        rotation_degrees: u16,
    ) -> Result<Self, PhysicalLayoutError> {
        if !physical.width.0.is_finite()
            || !physical.height.0.is_finite()
            || physical.width.0 <= 0.0
            || physical.height.0 <= 0.0
        {
            return Err(PhysicalLayoutError::InvalidPhysicalSize);
        }
        if native.width == 0 || native.height == 0 {
            return Err(PhysicalLayoutError::EmptyNativeSize);
        }

        let (width_px, height_px) = match rotation_degrees {
            90 | 270 => (f64::from(native.height), f64::from(native.width)),
            _ => (f64::from(native.width), f64::from(native.height)),
        };

        Ok(Self {
            x: width_px / (physical.width.0 / MM_PER_INCH),
            y: height_px / (physical.height.0 / MM_PER_INCH),
        })
    }
}

/// Outer panel rectangle including bezels.
#[must_use]
pub fn panel_rect(display: &Display) -> PhysicalRect {
    PhysicalRect {
        x: display.physical_origin.x,
        y: display.physical_origin.y,
        width: display.physical_size.width,
        height: display.physical_size.height,
    }
}

/// Active content rectangle inside the bezels.
pub fn content_rect(display: &Display) -> Result<PhysicalRect, PhysicalLayoutError> {
    display.bezel.validate()?;
    let width = display.physical_size.width.0 - display.bezel.horizontal().0;
    let height = display.physical_size.height.0 - display.bezel.vertical().0;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(PhysicalLayoutError::BezelExceedsPanel);
    }
    Ok(PhysicalRect {
        x: Millimeters(display.physical_origin.x.0 + display.bezel.left.0),
        y: Millimeters(display.physical_origin.y.0 + display.bezel.top.0),
        width: Millimeters(width),
        height: Millimeters(height),
    })
}

/// Axis-aligned bounding box of content rectangles for the given displays.
pub fn content_bounds(displays: &[Display]) -> Result<PhysicalRect, PhysicalLayoutError> {
    let mut iter = displays.iter();
    let first = iter.next().ok_or(PhysicalLayoutError::NoDisplays)?;
    let mut bounds = content_rect(first)?;
    for display in iter {
        bounds = bounds.union(content_rect(display)?);
    }
    if !bounds.is_valid() {
        return Err(PhysicalLayoutError::InvalidPhysicalSize);
    }
    Ok(bounds)
}

/// Builds a physical size from native pixels and a target PPI (same on both axes).
#[must_use]
pub fn physical_size_for_ppi(
    native: NativePixelSize,
    ppi: f64,
    rotation_degrees: u16,
) -> PhysicalSize {
    let ppi = if ppi.is_finite() && ppi > 0.0 {
        ppi
    } else {
        96.0
    };
    let (width_px, height_px) = match rotation_degrees {
        90 | 270 => (f64::from(native.height), f64::from(native.width)),
        _ => (f64::from(native.width), f64::from(native.height)),
    };
    PhysicalSize {
        width: Millimeters(width_px / ppi * MM_PER_INCH),
        height: Millimeters(height_px / ppi * MM_PER_INCH),
    }
}

/// Invalid physical layout geometry.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PhysicalLayoutError {
    /// At least one display is required.
    #[error("physical layout requires at least one display")]
    NoDisplays,
    /// Native pixel dimensions are empty.
    #[error("native pixel dimensions must be non-zero")]
    EmptyNativeSize,
    /// Physical dimensions are zero, negative, or not finite.
    #[error("physical dimensions must be positive finite values")]
    InvalidPhysicalSize,
    /// Bezel insets must be finite and non-negative.
    #[error("bezel insets must be finite and non-negative")]
    InvalidBezel,
    /// Bezels consume the entire panel.
    #[error("bezel insets exceed the panel size")]
    BezelExceedsPanel,
    /// Embedded display failed validation.
    #[error(transparent)]
    Display(#[from] DisplayValidationError),
}

/// Snaps a candidate origin so panel edges align with neighbors within `threshold_mm`.
#[must_use]
#[allow(clippy::similar_names)]
pub fn snap_origin(
    candidate: PhysicalPoint,
    moving: PhysicalSize,
    neighbors: &[(PhysicalPoint, PhysicalSize)],
    threshold_mm: f64,
) -> PhysicalPoint {
    let threshold = if threshold_mm.is_finite() && threshold_mm > 0.0 {
        threshold_mm
    } else {
        return candidate;
    };

    let mut x = candidate.x.0;
    let mut y = candidate.y.0;
    let moving_right = x + moving.width.0;
    let moving_bottom = y + moving.height.0;
    let moving_center_x = x + moving.width.0 / 2.0;
    let moving_center_y = y + moving.height.0 / 2.0;

    let mut nearest_x_delta = threshold;
    let mut nearest_y_delta = threshold;

    for (origin, size) in neighbors {
        let left = origin.x.0;
        let top = origin.y.0;
        let right = left + size.width.0;
        let bottom = top + size.height.0;
        let center_x = left + size.width.0 / 2.0;
        let center_y = top + size.height.0 / 2.0;

        for (from, to) in [
            (x, left),
            (x, right),
            (moving_right, left),
            (moving_right, right),
            (moving_center_x, center_x),
        ] {
            let delta = to - from;
            if delta.abs() <= nearest_x_delta {
                nearest_x_delta = delta.abs();
                x = candidate.x.0 + delta;
            }
        }

        for (from, to) in [
            (y, top),
            (y, bottom),
            (moving_bottom, top),
            (moving_bottom, bottom),
            (moving_center_y, center_y),
        ] {
            let delta = to - from;
            if delta.abs() <= nearest_y_delta {
                nearest_y_delta = delta.abs();
                y = candidate.y.0 + delta;
            }
        }
    }

    PhysicalPoint {
        x: Millimeters(x),
        y: Millimeters(y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DisplayId, LogicalRect, ScaleFactor};

    fn sample_display() -> Display {
        Display {
            id: DisplayId::from_u128(1),
            connector_name: Some("DP-1".into()),
            manufacturer: None,
            model: None,
            serial: None,
            logical_rect: LogicalRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            native_pixels: NativePixelSize {
                width: 1920,
                height: 1080,
            },
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
            bezel: BezelInsets::uniform(10.0),
            rotation_degrees: 0,
        }
    }

    #[test]
    fn content_rect_subtracts_bezels() {
        let rect = content_rect(&sample_display()).expect("content");
        assert!((rect.x.0 - 10.0).abs() < f64::EPSILON);
        assert!((rect.y.0 - 10.0).abs() < f64::EPSILON);
        assert!((rect.width.0 - 480.0).abs() < f64::EPSILON);
        assert!((rect.height.0 - 280.0).abs() < f64::EPSILON);
    }

    #[test]
    fn ppi_matches_panel_geometry() {
        let display = sample_display();
        let ppi = Ppi::from_display(
            display.native_pixels,
            display.physical_size,
            display.rotation_degrees,
        )
        .expect("ppi");
        let expected_x = 1920.0 / (500.0 / MM_PER_INCH);
        assert!((ppi.x - expected_x).abs() < 1e-9);
    }

    #[test]
    fn snap_aligns_adjacent_edges() {
        let neighbors = [(
            PhysicalPoint {
                x: Millimeters(500.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(400.0),
                height: Millimeters(300.0),
            },
        )];
        let snapped = snap_origin(
            PhysicalPoint {
                x: Millimeters(496.0),
                y: Millimeters(2.0),
            },
            PhysicalSize {
                width: Millimeters(400.0),
                height: Millimeters(300.0),
            },
            &neighbors,
            8.0,
        );
        assert!((snapped.x.0 - 500.0).abs() < 1e-9);
        assert!((snapped.y.0 - 0.0).abs() < 1e-9);
    }
}
