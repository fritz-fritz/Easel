// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic layout fixtures required by the quality strategy.

use crate::physical::{BezelInsets, PhysicalSizeSource};
use crate::{
    Display, DisplayArrangement, DisplayId, LogicalRect, Millimeters, NativePixelSize,
    PhysicalPoint, PhysicalSize, ScaleFactor,
};

#[allow(clippy::too_many_arguments)]
fn display(
    id: u128,
    connector: &str,
    native: NativePixelSize,
    logical: LogicalRect,
    scale: ScaleFactor,
    origin: PhysicalPoint,
    size: PhysicalSize,
    bezel: BezelInsets,
    rotation_degrees: u16,
) -> Display {
    Display {
        id: DisplayId::from_u128(id),
        connector_name: Some(connector.into()),
        manufacturer: Some("Fixture".into()),
        model: Some(connector.into()),
        serial: Some(format!("FIX-{id:04x}")),
        logical_rect: logical,
        native_pixels: native,
        scale_factor: scale,
        physical_size: size,
        physical_size_source: PhysicalSizeSource::Detected,
        physical_origin: origin,
        bezel,
        rotation_degrees,
    }
}

fn arrangement(displays: Vec<Display>) -> DisplayArrangement {
    DisplayArrangement::from_displays(displays).expect("fixture arrangement is valid")
}

/// One landscape display at the origin.
#[must_use]
pub fn one_display() -> DisplayArrangement {
    arrangement(vec![display(
        1,
        "eDP-1",
        NativePixelSize {
            width: 1920,
            height: 1080,
        },
        LogicalRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        },
        ScaleFactor::default(),
        PhysicalPoint {
            x: Millimeters(0.0),
            y: Millimeters(0.0),
        },
        PhysicalSize {
            width: Millimeters(500.0),
            height: Millimeters(280.0),
        },
        BezelInsets::default(),
        0,
    )])
}

/// Two equal displays in a horizontal row.
#[must_use]
pub fn two_equal_row() -> DisplayArrangement {
    arrangement(vec![
        display(
            2,
            "DP-1",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(280.0),
            },
            BezelInsets::uniform(5.0),
            0,
        ),
        display(
            3,
            "DP-2",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 1920,
                y: 0,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(510.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(280.0),
            },
            BezelInsets::uniform(5.0),
            0,
        ),
    ])
}

/// Layout whose logical coordinates include a negative origin.
#[must_use]
pub fn negative_logical_origin() -> DisplayArrangement {
    arrangement(vec![
        display(
            4,
            "DP-1",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(280.0),
            },
            BezelInsets::default(),
            0,
        ),
        display(
            5,
            "DP-2",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(500.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(280.0),
            },
            BezelInsets::default(),
            0,
        ),
    ])
}

/// Vertical stack of two displays.
#[must_use]
pub fn vertical_stack() -> DisplayArrangement {
    arrangement(vec![
        display(
            6,
            "DP-1",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(280.0),
            },
            BezelInsets::default(),
            0,
        ),
        display(
            7,
            "DP-2",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 0,
                y: 1080,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(290.0),
            },
            PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(280.0),
            },
            BezelInsets::default(),
            0,
        ),
    ])
}

/// T-shaped three-monitor layout.
#[must_use]
pub fn t_shaped() -> DisplayArrangement {
    arrangement(vec![
        display(
            8,
            "DP-1",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 0,
                y: 540,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(140.0),
            },
            PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(280.0),
            },
            BezelInsets::uniform(4.0),
            0,
        ),
        display(
            9,
            "DP-2",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 1920,
                y: 0,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(510.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(280.0),
            },
            BezelInsets::uniform(4.0),
            0,
        ),
        display(
            10,
            "DP-3",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 3840,
                y: 540,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(1020.0),
                y: Millimeters(140.0),
            },
            PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(280.0),
            },
            BezelInsets::uniform(4.0),
            0,
        ),
    ])
}

/// Portrait beside landscape.
#[must_use]
pub fn portrait_plus_landscape() -> DisplayArrangement {
    arrangement(vec![
        display(
            11,
            "DP-1",
            NativePixelSize {
                width: 1080,
                height: 1920,
            },
            LogicalRect {
                x: 0,
                y: 0,
                width: 1080,
                height: 1920,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(280.0),
                height: Millimeters(500.0),
            },
            BezelInsets::default(),
            90,
        ),
        display(
            12,
            "DP-2",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 1080,
                y: 420,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(290.0),
                y: Millimeters(110.0),
            },
            PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(280.0),
            },
            BezelInsets::default(),
            0,
        ),
    ])
}

/// Mixed fractional scale factors (125%, 150%, 200%).
#[must_use]
pub fn mixed_scale_factors() -> DisplayArrangement {
    arrangement(vec![
        display(
            13,
            "DP-1",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 0,
                y: 0,
                width: 1536,
                height: 864,
            },
            ScaleFactor::new(5, 4).expect("125%"),
            PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(500.0),
                height: Millimeters(280.0),
            },
            BezelInsets::default(),
            0,
        ),
        display(
            14,
            "DP-2",
            NativePixelSize {
                width: 2560,
                height: 1440,
            },
            LogicalRect {
                x: 1536,
                y: 0,
                width: 1707,
                height: 960,
            },
            ScaleFactor::new(3, 2).expect("150%"),
            PhysicalPoint {
                x: Millimeters(510.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(600.0),
                height: Millimeters(340.0),
            },
            BezelInsets::default(),
            0,
        ),
        display(
            15,
            "DP-3",
            NativePixelSize {
                width: 3840,
                height: 2160,
            },
            LogicalRect {
                x: 3243,
                y: 0,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::new(2, 1).expect("200%"),
            PhysicalPoint {
                x: Millimeters(1120.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(700.0),
                height: Millimeters(400.0),
            },
            BezelInsets::default(),
            0,
        ),
    ])
}

/// Same resolution, different physical sizes (PPI mismatch).
#[must_use]
pub fn different_physical_same_resolution() -> DisplayArrangement {
    arrangement(vec![
        display(
            16,
            "DP-1",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(480.0),
                height: Millimeters(270.0),
            },
            BezelInsets::uniform(6.0),
            0,
        ),
        display(
            17,
            "DP-2",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 1920,
                y: 0,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(500.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(600.0),
                height: Millimeters(340.0),
            },
            BezelInsets::uniform(6.0),
            0,
        ),
    ])
}

/// Same physical size, different resolutions.
#[must_use]
pub fn same_physical_different_resolution() -> DisplayArrangement {
    arrangement(vec![
        display(
            18,
            "DP-1",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(600.0),
                height: Millimeters(340.0),
            },
            BezelInsets::uniform(8.0),
            0,
        ),
        display(
            19,
            "DP-2",
            NativePixelSize {
                width: 3840,
                height: 2160,
            },
            LogicalRect {
                x: 1920,
                y: 0,
                width: 3840,
                height: 2160,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(620.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(600.0),
                height: Millimeters(340.0),
            },
            BezelInsets::uniform(8.0),
            0,
        ),
    ])
}

/// Bezel corrections on internal and outer edges.
#[must_use]
pub fn asymmetric_bezels() -> DisplayArrangement {
    arrangement(vec![
        display(
            20,
            "DP-1",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(520.0),
                height: Millimeters(300.0),
            },
            BezelInsets {
                left: Millimeters(12.0),
                top: Millimeters(10.0),
                right: Millimeters(4.0),
                bottom: Millimeters(14.0),
            },
            0,
        ),
        display(
            21,
            "DP-2",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 1920,
                y: 0,
                width: 1920,
                height: 1080,
            },
            ScaleFactor::default(),
            PhysicalPoint {
                x: Millimeters(520.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(520.0),
                height: Millimeters(300.0),
            },
            BezelInsets {
                left: Millimeters(4.0),
                top: Millimeters(10.0),
                right: Millimeters(12.0),
                bottom: Millimeters(14.0),
            },
            0,
        ),
    ])
}

/// All quality-strategy layout fixtures.
#[must_use]
pub fn all_layout_fixtures() -> Vec<(&'static str, DisplayArrangement)> {
    vec![
        ("one_display", one_display()),
        ("two_equal_row", two_equal_row()),
        ("negative_logical_origin", negative_logical_origin()),
        ("vertical_stack", vertical_stack()),
        ("t_shaped", t_shaped()),
        ("portrait_plus_landscape", portrait_plus_landscape()),
        ("mixed_scale_factors", mixed_scale_factors()),
        (
            "different_physical_same_resolution",
            different_physical_same_resolution(),
        ),
        (
            "same_physical_different_resolution",
            same_physical_different_resolution(),
        ),
        ("asymmetric_bezels", asymmetric_bezels()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::content_bounds;

    #[test]
    fn every_fixture_validates_and_has_content_bounds() {
        for (name, arrangement) in all_layout_fixtures() {
            arrangement
                .validate()
                .unwrap_or_else(|error| panic!("{name} invalid: {error}"));
            content_bounds(&arrangement.displays)
                .unwrap_or_else(|error| panic!("{name} bounds: {error}"));
        }
    }
}
