//! Deterministic render planning and local still raster execution.

#![forbid(unsafe_code)]

mod decode;
mod fit;
mod plan;
mod raster;

pub use decode::{DecodeError, DecodedImage, MAX_EDGE_PIXELS, MAX_TOTAL_PIXELS, decode_still};
pub use plan::{
    CompositionSettings, LetterboxColor, OutputOperation, OutputPlan, PixelRect, RENDERER_VERSION,
    RenderPlan, RenderPlanError, RenderPurpose, RenderRequest,
};
pub use raster::{RasterError, RasterJob, RasterOutput, atomic_write_png, render_operation};
