// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Platform-independent Easel domain model.

#![forbid(unsafe_code)]

mod arrangement;
mod asset;
mod display;
mod profile;

pub use arrangement::{
    ARRANGEMENT_SCHEMA_VERSION, ArrangementError, DisplayArrangement, DisplayEvidence,
    ObservedDisplay, approximate_physical_origin, match_displays,
};
pub use asset::{
    AssetId, AssetLicense, AssetLocation, Attribution, FrameRate, MediaAsset, MediaDimensions,
    MediaMetadata, ProviderAssetId,
};
pub use display::{
    Display, DisplayId, DisplayValidationError, LogicalRect, Millimeters, NativePixelSize,
    PhysicalPoint, PhysicalSize, ScaleFactor,
};
pub use profile::{
    FitMode, LoopMode, PROFILE_SCHEMA_VERSION, PlaybackPolicy, PresentationMode, Profile,
    ProfileId, ProfileValidationError,
};
