// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Display identity and coordinate-space types.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::physical::{BezelInsets, PhysicalSizeSource};

/// Stable Easel identity for a physical display.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DisplayId(Uuid);

impl DisplayId {
    /// Creates a new display identity.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Creates a stable display identity from a UUID byte layout.
    #[must_use]
    pub fn from_u128(value: u128) -> Self {
        Self(Uuid::from_u128(value))
    }

    /// Parses a hyphenated UUID string into a display identity.
    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(value.trim())?))
    }

    /// Returns the canonical hyphenated UUID string.
    #[must_use]
    pub fn to_hyphenated_string(self) -> String {
        self.0.hyphenated().to_string()
    }
}

impl Default for DisplayId {
    fn default() -> Self {
        Self::new()
    }
}

/// Native output dimensions in physical pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NativePixelSize {
    /// Output width.
    pub width: u32,
    /// Output height.
    pub height: u32,
}

/// Rectangle in compositor logical coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogicalRect {
    /// Logical x coordinate; may be negative.
    pub x: i32,
    /// Logical y coordinate; may be negative.
    pub y: i32,
    /// Logical width.
    pub width: u32,
    /// Logical height.
    pub height: u32,
}

/// Positive display scale factor represented without floating-point equality problems.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScaleFactor {
    numerator: u32,
    denominator: u32,
}

impl ScaleFactor {
    /// Creates a reduced positive scale factor.
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, DisplayValidationError> {
        if numerator == 0 || denominator == 0 {
            return Err(DisplayValidationError::InvalidScaleFactor);
        }
        let divisor = gcd(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    /// Returns the factor as an `f64` for calculations at an adapter boundary.
    #[must_use]
    pub fn as_f64(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }
}

impl Default for ScaleFactor {
    fn default() -> Self {
        Self {
            numerator: 1,
            denominator: 1,
        }
    }
}

const fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

/// Distance in millimeters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Millimeters(pub f64);

/// Physical display dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicalSize {
    /// Physical panel width.
    pub width: Millimeters,
    /// Physical panel height.
    pub height: Millimeters,
}

/// Display origin in user-calibrated physical layout space.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicalPoint {
    /// Horizontal position.
    pub x: Millimeters,
    /// Vertical position.
    pub y: Millimeters,
}

/// All coordinate spaces and identity evidence for one output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Display {
    /// Stable Easel identity.
    pub id: DisplayId,
    /// Current connector or platform display name.
    pub connector_name: Option<String>,
    /// EDID/platform manufacturer string.
    pub manufacturer: Option<String>,
    /// EDID/platform model string.
    pub model: Option<String>,
    /// EDID/platform serial string.
    pub serial: Option<String>,
    /// Compositor logical rectangle.
    pub logical_rect: LogicalRect,
    /// Native output dimensions.
    pub native_pixels: NativePixelSize,
    /// Logical-to-native scale.
    pub scale_factor: ScaleFactor,
    /// Detected or user-overridden physical dimensions.
    pub physical_size: PhysicalSize,
    /// Provenance for [`Self::physical_size`].
    #[serde(default)]
    pub physical_size_source: PhysicalSizeSource,
    /// User-calibrated physical origin.
    pub physical_origin: PhysicalPoint,
    /// Bezel thickness around the active panel area.
    #[serde(default)]
    pub bezel: BezelInsets,
    /// Clockwise rotation in degrees.
    pub rotation_degrees: u16,
}

impl Display {
    /// Validates invariants required by the planner.
    pub fn validate(&self) -> Result<(), DisplayValidationError> {
        if self.native_pixels.width == 0 || self.native_pixels.height == 0 {
            return Err(DisplayValidationError::EmptyNativeSize);
        }
        if self.logical_rect.width == 0 || self.logical_rect.height == 0 {
            return Err(DisplayValidationError::EmptyLogicalSize);
        }
        if !self.physical_size.width.0.is_finite()
            || !self.physical_size.height.0.is_finite()
            || self.physical_size.width.0 <= 0.0
            || self.physical_size.height.0 <= 0.0
        {
            return Err(DisplayValidationError::InvalidPhysicalSize);
        }
        for edge in [
            self.bezel.left.0,
            self.bezel.top.0,
            self.bezel.right.0,
            self.bezel.bottom.0,
        ] {
            if !edge.is_finite() || edge < 0.0 {
                return Err(DisplayValidationError::InvalidBezel);
            }
        }
        let content_w = self.physical_size.width.0 - self.bezel.left.0 - self.bezel.right.0;
        let content_h = self.physical_size.height.0 - self.bezel.top.0 - self.bezel.bottom.0;
        if content_w <= 0.0 || content_h <= 0.0 {
            return Err(DisplayValidationError::BezelExceedsPanel);
        }
        if !self.physical_origin.x.0.is_finite() || !self.physical_origin.y.0.is_finite() {
            return Err(DisplayValidationError::InvalidPhysicalOrigin);
        }
        if !matches!(self.rotation_degrees, 0 | 90 | 180 | 270) {
            return Err(DisplayValidationError::UnsupportedRotation(
                self.rotation_degrees,
            ));
        }
        Ok(())
    }
}

/// Invalid display model.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DisplayValidationError {
    /// Native pixel dimensions are empty.
    #[error("native pixel dimensions must be non-zero")]
    EmptyNativeSize,
    /// Logical dimensions are empty.
    #[error("logical dimensions must be non-zero")]
    EmptyLogicalSize,
    /// Physical dimensions are zero, negative, or not finite.
    #[error("physical dimensions must be positive finite values")]
    InvalidPhysicalSize,
    /// Physical origin coordinates must be finite.
    #[error("physical origin coordinates must be finite")]
    InvalidPhysicalOrigin,
    /// Bezel insets must be finite and non-negative.
    #[error("bezel insets must be finite and non-negative")]
    InvalidBezel,
    /// Bezels consume the entire panel.
    #[error("bezel insets exceed the panel size")]
    BezelExceedsPanel,
    /// Scale factors must be positive ratios.
    #[error("scale factor numerator and denominator must be non-zero")]
    InvalidScaleFactor,
    /// Only quarter-turn rotations are currently modeled.
    #[error("unsupported display rotation: {0} degrees")]
    UnsupportedRotation(u16),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_factor_is_reduced() {
        let factor = ScaleFactor::new(150, 100).expect("valid scale");
        assert_eq!(factor, ScaleFactor::new(3, 2).expect("valid scale"));
        assert!((factor.as_f64() - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn zero_scale_is_rejected() {
        assert_eq!(
            ScaleFactor::new(0, 1),
            Err(DisplayValidationError::InvalidScaleFactor)
        );
    }
}
