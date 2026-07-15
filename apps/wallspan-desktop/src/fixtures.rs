//! Dev fixture displays matching the Compose monitor preview layout.

use wallspan_core::{
    Display, DisplayId, LogicalRect, Millimeters, NativePixelSize, PhysicalPoint, PhysicalSize,
    ScaleFactor,
};

/// Stable identities used by the Compose preview until platform enumeration exists.
pub const LEFT_DISPLAY_ID: u128 = 0x1111_1111_1111_1111_1111_1111_1111_1111;
/// Center fixture display identity.
pub const CENTER_DISPLAY_ID: u128 = 0x2222_2222_2222_2222_2222_2222_2222_2222;
/// Right fixture display identity.
pub const RIGHT_DISPLAY_ID: u128 = 0x3333_3333_3333_3333_3333_3333_3333_3333;

/// Three-display arrangement matching the QML geometry labels.
#[must_use]
pub fn dev_displays() -> Vec<Display> {
    vec![
        display(
            LEFT_DISPLAY_ID,
            "DP-1",
            NativePixelSize {
                width: 2560,
                height: 1440,
            },
            LogicalRect {
                x: 0,
                y: 180,
                width: 2560,
                height: 1440,
            },
            PhysicalPoint {
                x: Millimeters(0.0),
                y: Millimeters(40.0),
            },
            PhysicalSize {
                width: Millimeters(600.0),
                height: Millimeters(340.0),
            },
        ),
        display(
            CENTER_DISPLAY_ID,
            "DP-2",
            NativePixelSize {
                width: 3840,
                height: 2160,
            },
            LogicalRect {
                x: 2560,
                y: 0,
                width: 3840,
                height: 2160,
            },
            PhysicalPoint {
                x: Millimeters(610.0),
                y: Millimeters(0.0),
            },
            PhysicalSize {
                width: Millimeters(700.0),
                height: Millimeters(400.0),
            },
        ),
        display(
            RIGHT_DISPLAY_ID,
            "DP-3",
            NativePixelSize {
                width: 1920,
                height: 1080,
            },
            LogicalRect {
                x: 6400,
                y: 360,
                width: 1920,
                height: 1080,
            },
            PhysicalPoint {
                x: Millimeters(1320.0),
                y: Millimeters(80.0),
            },
            PhysicalSize {
                width: Millimeters(530.0),
                height: Millimeters(300.0),
            },
        ),
    ]
}

/// Preview-sized copies of [`dev_displays`] so Compose refreshes stay responsive.
#[must_use]
#[allow(dead_code)]
pub fn preview_displays() -> Vec<Display> {
    const SCALE: u32 = 8;
    dev_displays()
        .into_iter()
        .map(|mut display| {
            display.native_pixels.width = (display.native_pixels.width / SCALE).max(1);
            display.native_pixels.height = (display.native_pixels.height / SCALE).max(1);
            display.logical_rect.width = (display.logical_rect.width / SCALE).max(1);
            display.logical_rect.height = (display.logical_rect.height / SCALE).max(1);
            display.logical_rect.x /= i32::try_from(SCALE).unwrap_or(1);
            display.logical_rect.y /= i32::try_from(SCALE).unwrap_or(1);
            display
        })
        .collect()
}

fn display(
    id: u128,
    connector: &str,
    native_pixels: NativePixelSize,
    logical_rect: LogicalRect,
    physical_origin: PhysicalPoint,
    physical_size: PhysicalSize,
) -> Display {
    Display {
        id: DisplayId::from_u128(id),
        connector_name: Some(connector.into()),
        manufacturer: Some("Fixture".into()),
        model: Some(connector.into()),
        serial: Some(format!("{id:032x}")),
        logical_rect,
        native_pixels,
        scale_factor: ScaleFactor::default(),
        physical_size,
        physical_origin,
        rotation_degrees: 0,
    }
}
