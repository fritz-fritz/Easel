//! Temporary helper used by the Stage 5 agent; kept as a documented example.

use easel_core::{DynamicStillKey, LocalTimeOfDay};
use easel_dynamic::{AppleMetadataFlavor, EncodeFrame, encode_dynamic_heic};
use image::{Rgba, RgbaImage};

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/easel-day.heic".into());
    let frames = vec![
        EncodeFrame {
            index: 0,
            key: DynamicStillKey::TimeOfDay {
                time: LocalTimeOfDay::new(6, 0).unwrap(),
            },
            image: RgbaImage::from_pixel(64, 48, Rgba([200, 80, 40, 255])),
        },
        EncodeFrame {
            index: 1,
            key: DynamicStillKey::TimeOfDay {
                time: LocalTimeOfDay::new(12, 0).unwrap(),
            },
            image: RgbaImage::from_pixel(64, 48, Rgba([40, 120, 200, 255])),
        },
        EncodeFrame {
            index: 2,
            key: DynamicStillKey::TimeOfDay {
                time: LocalTimeOfDay::new(18, 0).unwrap(),
            },
            image: RgbaImage::from_pixel(64, 48, Rgba([20, 20, 40, 255])),
        },
    ];
    encode_dynamic_heic(&frames, AppleMetadataFlavor::H24, &path).expect("encode");
    println!("{path}");
}
