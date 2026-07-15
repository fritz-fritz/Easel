//! Display identity and coordinate-space types.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Stable Wallspan identity for a physical display.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DisplayId(Uuid);

impl DisplayId {
    /// Creates a new display identity.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
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
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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
    /// Stable Wallspan identity.
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
    /// User-calibrated physical origin.
    pub physical_origin: PhysicalPoint,
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
