// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Apple Dynamic Desktop HEIC import and native per-display bundle planning.
//!
//! See `docs/adr/0006-apple-heic-dynamic-interchange.md`.

#![forbid(unsafe_code)]

mod bundle;
mod heic;
mod metadata;

pub use bundle::{
    DynamicBundlePlan, DynamicBundleTarget, NativeDynamicFormat, plan_per_display_bundles,
};
pub use heic::{ImportedDynamicDesktop, ImportedDynamicFrame, import_dynamic_heic};
pub use metadata::{
    AppleDesktopMetadata, AppleMetadataFlavor, parse_apple_desktop_from_xmp,
    parse_apple_desktop_plist, scrape_xmp_packet,
};
