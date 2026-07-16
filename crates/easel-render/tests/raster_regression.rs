// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Raster regression coverage using project-owned fixtures.

use std::fs;
use std::path::PathBuf;

use easel_core::{
    Display, DisplayId, FitMode, LogicalRect, Millimeters, NativePixelSize, PhysicalPoint,
    PhysicalSize, ScaleFactor,
};
use easel_render::{
    CompositionSettings, MAX_EDGE_PIXELS, RasterJob, RenderPurpose, RenderRequest, decode_still,
};
use image::{Rgba, RgbaImage};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn display(id: u128, width: u32, height: u32) -> Display {
    Display {
        id: DisplayId::from_u128(id),
        connector_name: Some(format!("TEST-{id}")),
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
        physical_origin: PhysicalPoint {
            x: Millimeters(0.0),
            y: Millimeters(0.0),
        },
        rotation_degrees: 0,
    }
}

#[test]
fn committed_fixture_covers_with_focal_bias() {
    let source = fixture_path("quadrants_32.png");
    let decoded = decode_still(&source).expect("decode fixture");
    assert_eq!(decoded.size().width, 32);
    assert_eq!(decoded.size().height, 32);

    let out_dir = std::env::temp_dir().join("easel-regression-focal");
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("outdir");

    let left_job = RasterJob {
        request: RenderRequest {
            source_path: source.clone(),
            displays: vec![display(10, 8, 16)],
            composition: CompositionSettings {
                fit_mode: FitMode::Cover,
                zoom: 1.0,
                focal_x: 0.0,
                focal_y: 0.5,
            },
            purpose: RenderPurpose::StaticWallpaper,
        },
        output_dir: out_dir.join("left"),
    };
    let right_job = RasterJob {
        request: RenderRequest {
            source_path: source,
            displays: vec![display(11, 8, 16)],
            composition: CompositionSettings {
                fit_mode: FitMode::Cover,
                zoom: 1.0,
                focal_x: 1.0,
                focal_y: 0.5,
            },
            purpose: RenderPurpose::StaticWallpaper,
        },
        output_dir: out_dir.join("right"),
    };

    let left = image::open(left_job.execute().expect("left")[0].path.as_path())
        .expect("open left")
        .to_rgba8();
    let right = image::open(right_job.execute().expect("right")[0].path.as_path())
        .expect("open right")
        .to_rgba8();

    // Portrait crop: left focal keeps the red quadrant, right focal keeps green.
    assert_eq!(*left.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
    assert_eq!(*right.get_pixel(0, 0), Rgba([0, 255, 0, 255]));
}

#[test]
fn contain_letterboxes_landscape_source() {
    let dir = std::env::temp_dir().join("easel-regression-contain");
    let _ = fs::create_dir_all(&dir);
    let source_path = dir.join("wide.png");
    let mut source = RgbaImage::new(40, 10);
    for pixel in source.pixels_mut() {
        *pixel = Rgba([200, 100, 50, 255]);
    }
    source.save(&source_path).expect("save");

    let job = RasterJob {
        request: RenderRequest {
            source_path,
            displays: vec![display(20, 20, 20)],
            composition: CompositionSettings {
                fit_mode: FitMode::Contain,
                zoom: 1.0,
                focal_x: 0.5,
                focal_y: 0.5,
            },
            purpose: RenderPurpose::StaticWallpaper,
        },
        output_dir: dir.join("out"),
    };
    let path = job.execute().expect("execute")[0].path.clone();
    let canvas = image::open(&path).expect("open").to_rgba8();
    // Top letterbox remains the default fill color.
    assert_eq!(*canvas.get_pixel(0, 0), Rgba([24, 24, 28, 255]));
    assert_eq!(*canvas.get_pixel(10, 10), Rgba([200, 100, 50, 255]));
}

#[test]
fn oversize_synthetic_image_is_rejected() {
    let dir = std::env::temp_dir().join("easel-regression-oversize");
    let _ = fs::create_dir_all(&dir);
    let source_path = dir.join("huge-header.png");

    // Patch a valid PNG IHDR so dimensions exceed the decode edge limit before pixels are read.
    let mut bytes = fs::read(fixture_path("quadrants_32.png")).expect("fixture");
    let width = (MAX_EDGE_PIXELS + 1).to_be_bytes();
    // IHDR data starts at byte 16 (8 signature + 4 length + 4 type).
    bytes[16..20].copy_from_slice(&width);
    // Recompute IHDR CRC over type+data (bytes 12..29).
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&bytes[12..29]);
    let crc = hasher.finalize().to_be_bytes();
    bytes[29..33].copy_from_slice(&crc);
    fs::write(&source_path, bytes).expect("write patched png");

    let err = decode_still(&source_path).expect_err("oversize rejected");
    assert!(
        matches!(err, easel_render::DecodeError::LimitExceeded { .. }),
        "unexpected decode error: {err}"
    );
}

#[test]
fn multi_display_job_writes_atomic_outputs() {
    let source = fixture_path("quadrants_32.png");
    let out_dir = std::env::temp_dir().join("easel-regression-multi");
    let _ = fs::remove_dir_all(&out_dir);

    let job = RasterJob {
        request: RenderRequest {
            source_path: source,
            displays: vec![display(40, 64, 36), display(41, 48, 48)],
            composition: CompositionSettings {
                fit_mode: FitMode::Cover,
                zoom: 1.25,
                focal_x: 0.3,
                focal_y: 0.7,
            },
            purpose: RenderPurpose::StaticWallpaper,
        },
        output_dir: out_dir,
    };
    let outputs = job.execute().expect("execute");
    assert_eq!(outputs.len(), 2);
    for output in outputs {
        assert!(output.path.is_file());
        assert!(!output.path.with_extension("png.part").exists());
        let sibling = output.path.parent().expect("parent").join(format!(
            "{}.part",
            output.path.file_name().and_then(|n| n.to_str()).unwrap()
        ));
        assert!(!sibling.exists());
    }
}
