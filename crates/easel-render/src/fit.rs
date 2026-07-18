// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic cover/contain/stretch/native geometry.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use easel_core::{FitMode, NativePixelSize};

use crate::plan::{CompositionSettings, PixelRect};

/// Computes source crop and destination placement for one output canvas.
#[must_use]
pub fn plan_fit(
    source: NativePixelSize,
    canvas: NativePixelSize,
    composition: &CompositionSettings,
) -> (PixelRect, PixelRect) {
    let zoom = composition.zoom.max(1.0);
    match composition.fit_mode {
        FitMode::Cover => plan_cover(
            source,
            canvas,
            zoom,
            composition.focal_x,
            composition.focal_y,
        ),
        FitMode::Contain => plan_contain(
            source,
            canvas,
            zoom,
            composition.focal_x,
            composition.focal_y,
        ),
        FitMode::Stretch => (PixelRect::full(source), PixelRect::full(canvas)),
        FitMode::Native => plan_native(source, canvas, composition.focal_x, composition.focal_y),
    }
}

fn plan_cover(
    source: NativePixelSize,
    canvas: NativePixelSize,
    zoom: f64,
    focal_x: f64,
    focal_y: f64,
) -> (PixelRect, PixelRect) {
    let scale = (f64::from(canvas.width) / f64::from(source.width))
        .max(f64::from(canvas.height) / f64::from(source.height))
        * zoom;
    let crop = focal_crop(source, canvas, scale, focal_x, focal_y);
    (crop, PixelRect::full(canvas))
}

fn plan_contain(
    source: NativePixelSize,
    canvas: NativePixelSize,
    zoom: f64,
    focal_x: f64,
    focal_y: f64,
) -> (PixelRect, PixelRect) {
    let scale = (f64::from(canvas.width) / f64::from(source.width))
        .min(f64::from(canvas.height) / f64::from(source.height))
        * zoom;
    let scaled_w = f64::from(source.width) * scale;
    let scaled_h = f64::from(source.height) * scale;

    if scaled_w <= f64::from(canvas.width) && scaled_h <= f64::from(canvas.height) {
        let dest_w = scaled_w.round().clamp(1.0, f64::from(canvas.width)) as u32;
        let dest_h = scaled_h.round().clamp(1.0, f64::from(canvas.height)) as u32;
        let dx = (i64::from(canvas.width) - i64::from(dest_w)) / 2;
        let dy = (i64::from(canvas.height) - i64::from(dest_h)) / 2;
        (
            PixelRect::full(source),
            PixelRect {
                x: dx as i32,
                y: dy as i32,
                width: dest_w,
                height: dest_h,
            },
        )
    } else {
        let crop = focal_crop(source, canvas, scale, focal_x, focal_y);
        (crop, PixelRect::full(canvas))
    }
}

fn plan_native(
    source: NativePixelSize,
    canvas: NativePixelSize,
    focal_x: f64,
    focal_y: f64,
) -> (PixelRect, PixelRect) {
    let crop_w = source.width.min(canvas.width);
    let crop_h = source.height.min(canvas.height);
    let max_x = source.width.saturating_sub(crop_w);
    let max_y = source.height.saturating_sub(crop_h);
    let x = (focal_x.clamp(0.0, 1.0) * f64::from(max_x)).round() as u32;
    let y = (focal_y.clamp(0.0, 1.0) * f64::from(max_y)).round() as u32;
    let dx = ((i64::from(canvas.width) - i64::from(crop_w)) / 2).max(0) as i32;
    let dy = ((i64::from(canvas.height) - i64::from(crop_h)) / 2).max(0) as i32;
    (
        PixelRect {
            x: x as i32,
            y: y as i32,
            width: crop_w,
            height: crop_h,
        },
        PixelRect {
            x: dx,
            y: dy,
            width: crop_w,
            height: crop_h,
        },
    )
}

fn focal_crop(
    source: NativePixelSize,
    canvas: NativePixelSize,
    scale: f64,
    focal_x: f64,
    focal_y: f64,
) -> PixelRect {
    let scale = scale.max(f64::EPSILON);
    let mut crop_w = (f64::from(canvas.width) / scale).min(f64::from(source.width));
    let mut crop_h = (f64::from(canvas.height) / scale).min(f64::from(source.height));
    crop_w = crop_w.max(1.0);
    crop_h = crop_h.max(1.0);

    let max_x = (f64::from(source.width) - crop_w).max(0.0);
    let max_y = (f64::from(source.height) - crop_h).max(0.0);
    let x = (focal_x.clamp(0.0, 1.0) * max_x).clamp(0.0, max_x);
    let y = (focal_y.clamp(0.0, 1.0) * max_y).clamp(0.0, max_y);

    let mut rect = PixelRect {
        x: x.floor() as i32,
        y: y.floor() as i32,
        width: crop_w.round().clamp(1.0, f64::from(source.width)) as u32,
        height: crop_h.round().clamp(1.0, f64::from(source.height)) as u32,
    };
    rect.clamp_to(source);
    rect
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::FitMode;

    fn size(width: u32, height: u32) -> NativePixelSize {
        NativePixelSize { width, height }
    }

    fn settings(fit_mode: FitMode, zoom: f64, focal_x: f64, focal_y: f64) -> CompositionSettings {
        CompositionSettings {
            fit_mode,
            layout_mode: easel_core::LayoutMode::Digital,
            zoom,
            focal_x,
            focal_y,
        }
    }

    #[test]
    fn cover_focal_left_vs_right_differs() {
        let source = size(200, 100);
        let canvas = size(100, 100);
        let left = plan_fit(source, canvas, &settings(FitMode::Cover, 1.0, 0.0, 0.5));
        let right = plan_fit(source, canvas, &settings(FitMode::Cover, 1.0, 1.0, 0.5));
        assert!(left.0.x < right.0.x);
        assert_eq!(left.0.width, right.0.width);
        assert_eq!(left.1, PixelRect::full(canvas));
    }

    #[test]
    fn contain_preserves_full_source() {
        let source = size(200, 100);
        let canvas = size(100, 100);
        let (crop, dest) = plan_fit(source, canvas, &settings(FitMode::Contain, 1.0, 0.5, 0.5));
        assert_eq!(crop, PixelRect::full(source));
        assert!(dest.width <= canvas.width);
        assert!(dest.height <= canvas.height);
        assert_eq!(dest.width, 100);
        assert_eq!(dest.height, 50);
    }

    #[test]
    fn stretch_uses_full_regions() {
        let source = size(40, 20);
        let canvas = size(10, 30);
        let (crop, dest) = plan_fit(source, canvas, &settings(FitMode::Stretch, 1.0, 0.2, 0.8));
        assert_eq!(crop, PixelRect::full(source));
        assert_eq!(dest, PixelRect::full(canvas));
    }

    #[test]
    fn native_respects_focal_and_canvas() {
        let source = size(40, 40);
        let canvas = size(20, 20);
        let left = plan_fit(source, canvas, &settings(FitMode::Native, 1.0, 0.0, 0.0));
        let right = plan_fit(source, canvas, &settings(FitMode::Native, 1.0, 1.0, 1.0));
        assert_eq!(
            left.0,
            PixelRect {
                x: 0,
                y: 0,
                width: 20,
                height: 20
            }
        );
        assert_eq!(
            right.0,
            PixelRect {
                x: 20,
                y: 20,
                width: 20,
                height: 20
            }
        );
    }
}
