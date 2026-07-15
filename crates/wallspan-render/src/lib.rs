//! Deterministic render planning. Raster execution will be added behind this boundary.

#![forbid(unsafe_code)]

use thiserror::Error;
use wallspan_core::{Display, DisplayId, DisplayValidationError, NativePixelSize};

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

/// Output requested for one physical display.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputPlan {
    /// Target display.
    pub display_id: DisplayId,
    /// Exact output dimensions.
    pub native_size: NativePixelSize,
}

/// Deterministic, serializable-in-spirit plan produced before decoding media bytes.
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
}

/// Render planning failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RenderPlanError {
    /// At least one display is required.
    #[error("render plan requires at least one display")]
    NoDisplays,
    /// A display record failed validation.
    #[error(transparent)]
    InvalidDisplay(#[from] DisplayValidationError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_layout_is_rejected() {
        assert_eq!(
            RenderPlan::for_displays(&[]),
            Err(RenderPlanError::NoDisplays)
        );
    }
}
