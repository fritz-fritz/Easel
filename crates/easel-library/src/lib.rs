// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Local folder index, library persistence, and acquisition cache.

#![forbid(unsafe_code)]

mod cache;
mod index;
mod store;
mod watch;

pub use cache::AcquisitionCache;
pub use index::{IndexedFolder, LocalIndexer, still_image_extension};
pub use store::{LibraryStore, LibraryStoreError};
pub use watch::{FolderWatchEvent, FolderWatcher};
