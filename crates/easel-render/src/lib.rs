// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Deterministic render planning and local still raster execution.

#![forbid(unsafe_code)]

mod decode;
mod fit;
mod plan;
mod raster;
mod resize;

pub use decode::{DecodeError, DecodedImage, MAX_EDGE_PIXELS, MAX_TOTAL_PIXELS, decode_still};
pub use plan::{
    CompositionSettings, LetterboxColor, OutputOperation, OutputPlan, PixelRect, RENDERER_VERSION,
    RenderPlan, RenderPlanError, RenderPurpose, RenderRequest,
};
pub use raster::{RasterError, RasterJob, RasterOutput, atomic_write_png, render_operation};
