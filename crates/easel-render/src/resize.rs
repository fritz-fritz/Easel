// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Cross-platform deterministic resampling.
//!
//! The `image` crate's `FilterType::Lanczos3` calls platform `libm` for `sin`, which
//! produces ±1 LSB channel differences between MSVC and Unix for some scale factors.
//! This module uses the pure-Rust `libm` crate so apply-payload rasters stay
//! byte-identical across CI OS runners.
//!
//! Kernel weights are precomputed once per axis so large wallpapers do not recompute
//! `sin` for every output pixel.

use image::{Rgba, RgbaImage};

const LANCZOS_RADIUS: f64 = 3.0;

/// One weighted source sample contributing to an output coordinate.
#[derive(Clone, Copy, Debug)]
struct SampleWeight {
    index: u32,
    weight: f64,
}

/// Resizes `source` to `out_width`×`out_height` with a portable Lanczos-3 kernel.
#[must_use]
pub fn resize_lanczos3(source: &RgbaImage, out_width: u32, out_height: u32) -> RgbaImage {
    if out_width == 0 || out_height == 0 {
        return RgbaImage::new(out_width, out_height);
    }
    if source.width() == out_width && source.height() == out_height {
        return source.clone();
    }

    let horizontal = if source.width() == out_width {
        source.clone()
    } else {
        let contributions = build_contributions(source.width(), out_width);
        resize_horizontal(source, out_width, &contributions)
    };

    if horizontal.height() == out_height {
        horizontal
    } else {
        let contributions = build_contributions(horizontal.height(), out_height);
        resize_vertical(&horizontal, out_height, &contributions)
    }
}

fn build_contributions(in_extent: u32, out_extent: u32) -> Vec<Vec<SampleWeight>> {
    let scale = f64::from(in_extent) / f64::from(out_extent);
    let filter_scale = scale.max(1.0);
    let support = LANCZOS_RADIUS * filter_scale;
    let max_index = in_extent.saturating_sub(1);
    let mut contributions = Vec::with_capacity(out_extent as usize);

    for out in 0..out_extent {
        let center = (f64::from(out) + 0.5) * scale - 0.5;
        let first = floor_clamped(center - support, max_index);
        let last = ceil_clamped(center + support, max_index);
        let mut samples = Vec::new();
        let mut weight_sum = 0.0_f64;
        for sample in first..=last {
            let weight = lanczos3((f64::from(sample) - center) / filter_scale);
            if weight == 0.0 {
                continue;
            }
            weight_sum += weight;
            samples.push(SampleWeight {
                index: sample,
                weight,
            });
        }
        if weight_sum.abs() > f64::EPSILON {
            for sample in &mut samples {
                sample.weight /= weight_sum;
            }
        } else {
            samples.clear();
        }
        contributions.push(samples);
    }
    contributions
}

fn resize_horizontal(
    source: &RgbaImage,
    out_width: u32,
    contributions: &[Vec<SampleWeight>],
) -> RgbaImage {
    let height = source.height();
    let mut output = RgbaImage::new(out_width, height);
    for y in 0..height {
        for (x, samples) in contributions.iter().enumerate() {
            output.put_pixel(
                u32::try_from(x).unwrap_or(0),
                y,
                gather_samples(samples, |index| *source.get_pixel(index, y)),
            );
        }
    }
    output
}

fn resize_vertical(
    source: &RgbaImage,
    out_height: u32,
    contributions: &[Vec<SampleWeight>],
) -> RgbaImage {
    let width = source.width();
    let mut output = RgbaImage::new(width, out_height);
    for (y, samples) in contributions.iter().enumerate() {
        for x in 0..width {
            output.put_pixel(
                x,
                u32::try_from(y).unwrap_or(0),
                gather_samples(samples, |index| *source.get_pixel(x, index)),
            );
        }
    }
    output
}

fn gather_samples(samples: &[SampleWeight], pixel_at: impl Fn(u32) -> Rgba<u8>) -> Rgba<u8> {
    if samples.is_empty() {
        return Rgba([0, 0, 0, 255]);
    }
    let mut acc = [0.0_f64; 4];
    for sample in samples {
        let pixel = pixel_at(sample.index);
        for (channel, total) in acc.iter_mut().enumerate() {
            *total += f64::from(pixel.0[channel]) * sample.weight;
        }
    }
    Rgba([
        round_u8(acc[0]),
        round_u8(acc[1]),
        round_u8(acc[2]),
        round_u8(acc[3]),
    ])
}

fn floor_clamped(value: f64, max_index: u32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let floored = value.floor();
    if floored >= f64::from(max_index) {
        max_index
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            floored as u32
        }
    }
}

fn ceil_clamped(value: f64, max_index: u32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let ceiled = value.ceil();
    if ceiled >= f64::from(max_index) {
        max_index
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            ceiled as u32
        }
    }
}

fn lanczos3(x: f64) -> f64 {
    let ax = x.abs();
    if ax < f64::EPSILON {
        return 1.0;
    }
    if ax >= LANCZOS_RADIUS {
        return 0.0;
    }
    sinc(x) * sinc(x / LANCZOS_RADIUS)
}

fn sinc(x: f64) -> f64 {
    if x.abs() < f64::EPSILON {
        return 1.0;
    }
    let pix = std::f64::consts::PI * x;
    libm::sin(pix) / pix
}

fn round_u8(value: f64) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    let clamped = value.clamp(0.0, 255.0);
    // Non-negative values only: round half away from zero via floor(x + 0.5).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (clamped + 0.5).floor() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_resize_clones() {
        let mut source = RgbaImage::new(3, 2);
        source.put_pixel(0, 0, Rgba([1, 2, 3, 255]));
        source.put_pixel(2, 1, Rgba([9, 8, 7, 255]));
        let out = resize_lanczos3(&source, 3, 2);
        assert_eq!(out, source);
    }

    #[test]
    fn repeated_resize_is_stable() {
        let mut source = RgbaImage::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                let v = u8::try_from(x * 17 + y * 3).unwrap_or(255);
                source.put_pixel(x, y, Rgba([v, 255 - v, 128, 255]));
            }
        }
        let first = resize_lanczos3(&source, 240, 135);
        let second = resize_lanczos3(&source, 240, 135);
        assert_eq!(first.as_raw(), second.as_raw());
    }

    #[test]
    fn upscale_stays_inside_gamut() {
        let source = RgbaImage::from_pixel(2, 2, Rgba([0, 255, 0, 255]));
        let out = resize_lanczos3(&source, 32, 18);
        for pixel in out.pixels() {
            assert_eq!(pixel.0[3], 255);
        }
    }
}
