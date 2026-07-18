// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Versioned display arrangements and stable identity matching.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::physical::{BezelInsets, PhysicalSizeSource, snap_origin};
use crate::{
    Display, DisplayId, DisplayValidationError, LogicalRect, Millimeters, NativePixelSize,
    PhysicalPoint, PhysicalSize, ScaleFactor,
};

/// Current serialized arrangement schema.
pub const ARRANGEMENT_SCHEMA_VERSION: u16 = 1;

/// Persisted multi-display layout with stable Easel identities.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayArrangement {
    /// Serialized schema version.
    pub schema_version: u16,
    /// Ordered displays in physical/logical layout order.
    pub displays: Vec<Display>,
}

impl DisplayArrangement {
    /// Creates an empty arrangement at the current schema version.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            schema_version: ARRANGEMENT_SCHEMA_VERSION,
            displays: Vec::new(),
        }
    }

    /// Creates an arrangement from validated displays.
    pub fn from_displays(displays: Vec<Display>) -> Result<Self, ArrangementError> {
        let arrangement = Self {
            schema_version: ARRANGEMENT_SCHEMA_VERSION,
            displays,
        };
        arrangement.validate()?;
        Ok(arrangement)
    }

    /// Validates schema version and every display record.
    pub fn validate(&self) -> Result<(), ArrangementError> {
        if self.schema_version != ARRANGEMENT_SCHEMA_VERSION {
            return Err(ArrangementError::UnsupportedSchema(self.schema_version));
        }
        for display in &self.displays {
            display.validate()?;
        }
        Ok(())
    }

    /// Returns a mutable display by stable identity.
    pub fn display_mut(&mut self, id: DisplayId) -> Option<&mut Display> {
        self.displays.iter_mut().find(|display| display.id == id)
    }

    /// Moves a display origin in physical space, optionally snapping to neighbors.
    pub fn move_display(
        &mut self,
        id: DisplayId,
        origin: PhysicalPoint,
        snap_threshold_mm: Option<f64>,
    ) -> Result<(), ArrangementError> {
        let moving_size = self
            .displays
            .iter()
            .find(|display| display.id == id)
            .map(|display| display.physical_size)
            .ok_or(ArrangementError::UnknownDisplay(id))?;

        let neighbors: Vec<(PhysicalPoint, PhysicalSize)> = self
            .displays
            .iter()
            .filter(|display| display.id != id)
            .map(|display| (display.physical_origin, display.physical_size))
            .collect();

        let snapped = snap_threshold_mm.map_or(origin, |threshold| {
            snap_origin(origin, moving_size, &neighbors, threshold)
        });

        let display = self
            .display_mut(id)
            .ok_or(ArrangementError::UnknownDisplay(id))?;
        display.physical_origin = snapped;
        display.validate()?;
        Ok(())
    }

    /// Overrides the physical size for a display and marks it as user-authored.
    pub fn override_physical_size(
        &mut self,
        id: DisplayId,
        size: PhysicalSize,
    ) -> Result<(), ArrangementError> {
        let display = self
            .display_mut(id)
            .ok_or(ArrangementError::UnknownDisplay(id))?;
        display.physical_size = size;
        display.physical_size_source = PhysicalSizeSource::UserOverride;
        display.validate()?;
        Ok(())
    }

    /// Sets bezel insets for a display.
    pub fn set_bezel(&mut self, id: DisplayId, bezel: BezelInsets) -> Result<(), ArrangementError> {
        let display = self
            .display_mut(id)
            .ok_or(ArrangementError::UnknownDisplay(id))?;
        display.bezel = bezel;
        display.validate()?;
        Ok(())
    }

    /// Sets clockwise rotation for a display.
    pub fn set_rotation(
        &mut self,
        id: DisplayId,
        rotation_degrees: u16,
    ) -> Result<(), ArrangementError> {
        let display = self
            .display_mut(id)
            .ok_or(ArrangementError::UnknownDisplay(id))?;
        display.rotation_degrees = rotation_degrees;
        display.validate()?;
        Ok(())
    }
}

/// Probe observation used to rematch a physical output across sessions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayEvidence {
    /// Connector or platform display name.
    pub connector_name: Option<String>,
    /// EDID/platform manufacturer string.
    pub manufacturer: Option<String>,
    /// EDID/platform model string.
    pub model: Option<String>,
    /// EDID/platform serial string.
    pub serial: Option<String>,
    /// Native output dimensions.
    pub native_pixels: NativePixelSize,
}

impl DisplayEvidence {
    /// Builds matching evidence from a full display record.
    #[must_use]
    pub fn from_display(display: &Display) -> Self {
        Self {
            connector_name: display.connector_name.clone(),
            manufacturer: display.manufacturer.clone(),
            model: display.model.clone(),
            serial: display.serial.clone(),
            native_pixels: display.native_pixels,
        }
    }
}

/// Geometry and identity fields discovered from the platform this session.
#[derive(Clone, Debug, PartialEq)]
pub struct ObservedDisplay {
    /// Identity evidence used for rematching.
    pub evidence: DisplayEvidence,
    /// Compositor logical rectangle.
    pub logical_rect: LogicalRect,
    /// Logical-to-native scale.
    pub scale_factor: ScaleFactor,
    /// Detected physical dimensions.
    pub physical_size: PhysicalSize,
    /// Estimated physical origin for this session.
    pub physical_origin: PhysicalPoint,
    /// Clockwise rotation in degrees.
    pub rotation_degrees: u16,
}

impl ObservedDisplay {
    /// Converts an observation into a domain display with the given stable id.
    #[must_use]
    pub fn into_display(self, id: DisplayId) -> Display {
        Display {
            id,
            connector_name: self.evidence.connector_name,
            manufacturer: self.evidence.manufacturer,
            model: self.evidence.model,
            serial: self.evidence.serial,
            logical_rect: self.logical_rect,
            native_pixels: self.evidence.native_pixels,
            scale_factor: self.scale_factor,
            physical_size: self.physical_size,
            physical_size_source: PhysicalSizeSource::Detected,
            physical_origin: self.physical_origin,
            bezel: BezelInsets::default(),
            rotation_degrees: self.rotation_degrees,
        }
    }

    /// Converts an observation while preserving user calibration from a prior match.
    #[must_use]
    pub fn into_display_preserving(self, previous: &Display) -> Display {
        let mut display = self.into_display(previous.id);
        display.physical_origin = previous.physical_origin;
        display.bezel = previous.bezel;
        display.rotation_degrees = previous.rotation_degrees;
        if previous.physical_size_source == PhysicalSizeSource::UserOverride {
            display.physical_size = previous.physical_size;
            display.physical_size_source = PhysicalSizeSource::UserOverride;
        }
        display
    }
}

/// Rematches observed outputs against a previously persisted arrangement.
///
/// Prefer manufacturer + model + serial. Fall back to connector + native size.
/// Unmatched observations receive new identities. Matched displays keep user
/// physical origin, bezel, rotation, and size overrides.
#[must_use]
pub fn match_displays(
    previous: &DisplayArrangement,
    observed: Vec<ObservedDisplay>,
) -> DisplayArrangement {
    let mut remaining: Vec<&Display> = previous.displays.iter().collect();
    let mut matched = Vec::with_capacity(observed.len());

    for observation in observed {
        let display = match take_best_match(&mut remaining, &observation.evidence) {
            Some(previous_display) => observation.into_display_preserving(previous_display),
            None => observation.into_display(DisplayId::new()),
        };
        matched.push(display);
    }

    DisplayArrangement {
        schema_version: ARRANGEMENT_SCHEMA_VERSION,
        displays: matched,
    }
}

fn take_best_match<'a>(
    remaining: &mut Vec<&'a Display>,
    evidence: &DisplayEvidence,
) -> Option<&'a Display> {
    if let Some(index) = remaining.iter().position(|display| {
        strong_identity_match(&DisplayEvidence::from_display(display), evidence)
    }) {
        return Some(remaining.remove(index));
    }

    if let Some(index) = remaining
        .iter()
        .position(|display| weak_identity_match(&DisplayEvidence::from_display(display), evidence))
    {
        return Some(remaining.remove(index));
    }

    None
}

fn strong_identity_match(left: &DisplayEvidence, right: &DisplayEvidence) -> bool {
    match (
        normalize_opt(left.manufacturer.as_deref()),
        normalize_opt(left.model.as_deref()),
        normalize_opt(left.serial.as_deref()),
        normalize_opt(right.manufacturer.as_deref()),
        normalize_opt(right.model.as_deref()),
        normalize_opt(right.serial.as_deref()),
    ) {
        (Some(lm), Some(lmodel), Some(ls), Some(rm), Some(rmodel), Some(rs)) => {
            lm == rm && lmodel == rmodel && ls == rs
        }
        _ => false,
    }
}

fn weak_identity_match(left: &DisplayEvidence, right: &DisplayEvidence) -> bool {
    let Some(left_connector) = normalize_opt(left.connector_name.as_deref()) else {
        return false;
    };
    let Some(right_connector) = normalize_opt(right.connector_name.as_deref()) else {
        return false;
    };
    left_connector == right_connector && left.native_pixels == right.native_pixels
}

fn normalize_opt(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

/// Invalid arrangement model or unsupported schema.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ArrangementError {
    /// Schema version is not supported by this build.
    #[error("unsupported arrangement schema version: {0}")]
    UnsupportedSchema(u16),
    /// An embedded display failed validation.
    #[error(transparent)]
    Display(#[from] DisplayValidationError),
    /// The requested display is not part of this arrangement.
    #[error("unknown display in arrangement: {0:?}")]
    UnknownDisplay(DisplayId),
}

/// Convenience physical origin at the logical origin scaled by an approximate PPI.
#[must_use]
pub fn approximate_physical_origin(logical: LogicalRect, physical: PhysicalSize) -> PhysicalPoint {
    let ppi_x = if logical.width == 0 {
        96.0
    } else {
        f64::from(logical.width) / (physical.width.0 / 25.4)
    };
    let ppi_y = if logical.height == 0 {
        96.0
    } else {
        f64::from(logical.height) / (physical.height.0 / 25.4)
    };
    PhysicalPoint {
        x: Millimeters(f64::from(logical.x) / ppi_x * 25.4),
        y: Millimeters(f64::from(logical.y) / ppi_y * 25.4),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_display(id: u128, connector: &str, serial: &str, width: u32) -> Display {
        Display {
            id: DisplayId::from_u128(id),
            connector_name: Some(connector.into()),
            manufacturer: Some("Acme".into()),
            model: Some("Ultra".into()),
            serial: Some(serial.into()),
            logical_rect: LogicalRect {
                x: 0,
                y: 0,
                width,
                height: 1080,
            },
            native_pixels: NativePixelSize {
                width,
                height: 1080,
            },
            scale_factor: ScaleFactor::default(),
            physical_size: PhysicalSize {
                width: Millimeters(600.0),
                height: Millimeters(340.0),
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

    fn observation_from(display: &Display) -> ObservedDisplay {
        ObservedDisplay {
            evidence: DisplayEvidence::from_display(display),
            logical_rect: display.logical_rect,
            scale_factor: display.scale_factor,
            physical_size: display.physical_size,
            physical_origin: display.physical_origin,
            rotation_degrees: display.rotation_degrees,
        }
    }

    #[test]
    fn strong_match_preserves_id_when_connector_changes() {
        let previous =
            DisplayArrangement::from_displays(vec![sample_display(1, "DP-1", "SN-1", 1920)])
                .expect("valid");
        let mut observed = observation_from(&previous.displays[0]);
        observed.evidence.connector_name = Some("HDMI-A-1".into());
        observed.logical_rect.x = 100;

        let matched = match_displays(&previous, vec![observed]);
        assert_eq!(matched.displays[0].id, DisplayId::from_u128(1));
        assert_eq!(
            matched.displays[0].connector_name.as_deref(),
            Some("HDMI-A-1")
        );
        assert_eq!(matched.displays[0].logical_rect.x, 100);
    }

    #[test]
    fn weak_match_preserves_id_without_edid() {
        let mut display = sample_display(2, "eDP-1", "", 2560);
        display.manufacturer = None;
        display.model = None;
        display.serial = None;
        let previous = DisplayArrangement::from_displays(vec![display.clone()]).expect("valid");
        let mut observed = observation_from(&display);
        observed.logical_rect.y = 40;

        let matched = match_displays(&previous, vec![observed]);
        assert_eq!(matched.displays[0].id, DisplayId::from_u128(2));
    }

    #[test]
    fn unmatched_observation_gets_new_id() {
        let previous =
            DisplayArrangement::from_displays(vec![sample_display(3, "DP-1", "SN-OLD", 1920)])
                .expect("valid");
        let mut observed = observation_from(&previous.displays[0]);
        observed.evidence.serial = Some("SN-NEW".into());
        observed.evidence.connector_name = Some("DP-2".into());
        observed.evidence.native_pixels.width = 3840;

        let matched = match_displays(&previous, vec![observed]);
        assert_ne!(matched.displays[0].id, DisplayId::from_u128(3));
    }

    #[test]
    fn empty_previous_assigns_fresh_ids() {
        let observed = observation_from(&sample_display(9, "DP-1", "SN-1", 1920));
        let matched = match_displays(&DisplayArrangement::empty(), vec![observed]);
        assert_eq!(matched.displays.len(), 1);
        assert_ne!(matched.displays[0].id, DisplayId::from_u128(9));
    }

    #[test]
    fn rematch_preserves_user_calibration() {
        let mut previous_display = sample_display(1, "DP-1", "SN-1", 1920);
        previous_display.physical_origin = PhysicalPoint {
            x: Millimeters(120.0),
            y: Millimeters(40.0),
        };
        previous_display.bezel = BezelInsets::uniform(8.0);
        previous_display.physical_size = PhysicalSize {
            width: Millimeters(580.0),
            height: Millimeters(330.0),
        };
        previous_display.physical_size_source = PhysicalSizeSource::UserOverride;
        let previous =
            DisplayArrangement::from_displays(vec![previous_display.clone()]).expect("valid");

        let mut observed = observation_from(&previous_display);
        observed.physical_origin = PhysicalPoint {
            x: Millimeters(0.0),
            y: Millimeters(0.0),
        };
        observed.physical_size = PhysicalSize {
            width: Millimeters(600.0),
            height: Millimeters(340.0),
        };

        let matched = match_displays(&previous, vec![observed]);
        assert!((matched.displays[0].physical_origin.x.0 - 120.0).abs() < f64::EPSILON);
        assert!((matched.displays[0].bezel.left.0 - 8.0).abs() < f64::EPSILON);
        assert!((matched.displays[0].physical_size.width.0 - 580.0).abs() < f64::EPSILON);
        assert_eq!(
            matched.displays[0].physical_size_source,
            PhysicalSizeSource::UserOverride
        );
    }

    #[test]
    fn move_display_snaps_to_neighbor() {
        let left = sample_display(1, "DP-1", "SN-1", 1920);
        let mut right = sample_display(2, "DP-2", "SN-2", 1920);
        right.physical_origin = PhysicalPoint {
            x: Millimeters(610.0),
            y: Millimeters(0.0),
        };
        let mut arrangement = DisplayArrangement::from_displays(vec![left, right]).expect("valid");
        arrangement
            .move_display(
                DisplayId::from_u128(2),
                PhysicalPoint {
                    x: Millimeters(604.0),
                    y: Millimeters(3.0),
                },
                Some(10.0),
            )
            .expect("move");
        assert!((arrangement.displays[1].physical_origin.x.0 - 600.0).abs() < 1e-9);
        assert!((arrangement.displays[1].physical_origin.y.0 - 0.0).abs() < 1e-9);
    }
}
