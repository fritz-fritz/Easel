// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Writes per-display apply-payload PNGs for visual review artifacts.

use std::env;
use std::path::PathBuf;

use easel_core::{
    BezelInsets, Display, DisplayId, FitMode, LayoutMode, LogicalRect, Millimeters,
    NativePixelSize, PhysicalPoint, PhysicalSize, PhysicalSizeSource, Profile, ScaleFactor,
};
use easel_render::{CompositionSettings, RasterJob, RenderPurpose, RenderRequest};

fn fixture_displays() -> Vec<Display> {
    vec![
        display(
            0x1111_1111_1111_1111_1111_1111_1111_1111,
            "DP-1",
            320,
            180,
            0,
            45,
        ),
        display(
            0x2222_2222_2222_2222_2222_2222_2222_2222,
            "DP-2",
            480,
            270,
            320,
            0,
        ),
        display(
            0x3333_3333_3333_3333_3333_3333_3333_3333,
            "DP-3",
            240,
            135,
            800,
            90,
        ),
    ]
}

fn display(id: u128, connector: &str, width: u32, height: u32, x: i32, y: i32) -> Display {
    Display {
        id: DisplayId::from_u128(id),
        connector_name: Some(connector.into()),
        manufacturer: Some("Fixture".into()),
        model: Some(connector.into()),
        serial: Some(format!("{id:032x}")),
        logical_rect: LogicalRect {
            x,
            y,
            width,
            height,
        },
        native_pixels: NativePixelSize { width, height },
        scale_factor: ScaleFactor::default(),
        physical_size: PhysicalSize {
            width: Millimeters(f64::from(width) / 96.0 * 25.4),
            height: Millimeters(f64::from(height) / 96.0 * 25.4),
        },
        physical_size_source: PhysicalSizeSource::Detected,
        physical_origin: PhysicalPoint {
            x: Millimeters(f64::from(x) / 96.0 * 25.4),
            y: Millimeters(f64::from(y) / 96.0 * 25.4),
        },
        bezel: BezelInsets::uniform(2.0),
        rotation_degrees: 0,
    }
}

#[test]
fn write_apply_payload_visual_artifacts() {
    // CI sets EASEL_VISUAL_OUTDIR. Skip locally so `cargo test` does not leave
    // PNGs under a shared temp path or cross-contaminate runs.
    let Some(out_dir) = env::var_os("EASEL_VISUAL_OUTDIR").map(PathBuf::from) else {
        eprintln!("skipping write_apply_payload_visual_artifacts: EASEL_VISUAL_OUTDIR unset");
        return;
    };

    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/quadrants_32.png");
    std::fs::create_dir_all(&out_dir).expect("visual out dir");

    let displays = fixture_displays();
    let mut profile = Profile::new("visual");
    profile.fit_mode = FitMode::Cover;
    profile.layout_mode = LayoutMode::PhysicalSpan;
    profile.displays = displays.iter().map(|display| display.id).collect();

    let outputs = RasterJob {
        request: RenderRequest {
            source_path: source,
            displays,
            composition: CompositionSettings::from_profile(&profile),
            purpose: RenderPurpose::StaticWallpaper,
        },
        output_dir: out_dir.clone(),
    }
    .execute()
    .expect("raster");

    assert_eq!(outputs.len(), 3);
    for (index, output) in outputs.iter().enumerate() {
        let named = out_dir.join(format!("apply-display-{index}.png"));
        std::fs::copy(&output.path, &named).expect("copy visual artifact");
        assert!(named.is_file());
    }
}
