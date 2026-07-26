// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Apple Dynamic Desktop HEIC import/export and native per-display bundles.
//!
//! See `docs/adr/0006-apple-heic-dynamic-interchange.md`.

#![forbid(unsafe_code)]

mod bundle;
mod encode;
mod heic;
mod metadata;
mod persist;
mod plasma_day_night;

pub use bundle::{
    BundleEncodeError, BundlePlanError, DynamicBundlePlan, DynamicBundleTarget,
    EncodedDynamicBundle, NativeDynamicFormat, cached_bundle_path, encode_per_display_bundles,
    plan_per_display_bundles, preferred_native_format, prefers_still_frame_host,
};
pub use encode::{EncodeFrame, HeicEncodeError, encode_dynamic_heic, encode_still_set_heic};
pub use heic::{
    HeicImportError, ImportedDynamicDesktop, ImportedDynamicFrame, import_dynamic_heic,
};
pub use metadata::{
    AppleDesktopMetadata, AppleMetadataFlavor, MetadataError, apple_keys_from_still_set,
    build_apple_desktop_plist, build_apple_xmp, parse_apple_desktop_from_xmp,
    parse_apple_desktop_plist, scrape_xmp_packet,
};
pub use persist::{PersistError, PersistedDynamicImport, persist_imported_desktop};
pub use plasma_day_night::{
    PlasmaDayNightError, PlasmaDayNightPackage, appearance_frames_from_set,
    write_plasma_day_night_package,
};
