// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Local folder index, library persistence, and acquisition cache.

#![forbid(unsafe_code)]

mod cache;
mod index;
mod probe;
mod store;
mod watch;

pub use cache::AcquisitionCache;
pub use index::{IndexOutcome, IndexedFolder, LocalIndexer};
pub use probe::{
    ProbeError, animated_image_extension, local_media_extension, poster_path_for_asset,
    probe_local_media, still_image_extension, video_extension, write_poster_for_asset,
};
pub use store::{LibraryStore, LibraryStoreError};
pub use watch::{FolderWatchEvent, FolderWatcher};
