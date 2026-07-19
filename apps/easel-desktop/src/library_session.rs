// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared on-disk library store and acquisition cache locations.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use directories::ProjectDirs;
use easel_library::{AcquisitionCache, LibraryStore};

/// Opens the process-wide library store.
pub fn library_store() -> Result<std::sync::MutexGuard<'static, LibraryStore>, String> {
    static STORE: OnceLock<Mutex<LibraryStore>> = OnceLock::new();
    let mutex = STORE.get_or_init(|| {
        let path = data_dir().join("library.db");
        Mutex::new(LibraryStore::open(path).expect("open library store"))
    });
    mutex
        .lock()
        .map_err(|_| "library store lock poisoned".into())
}

/// Opens the process-wide acquisition cache.
pub fn acquisition_cache() -> Result<std::sync::MutexGuard<'static, AcquisitionCache>, String> {
    static CACHE: OnceLock<Mutex<AcquisitionCache>> = OnceLock::new();
    let mutex = CACHE.get_or_init(|| {
        let path = cache_dir().join("acquisitions");
        Mutex::new(AcquisitionCache::new(path).expect("open acquisition cache"))
    });
    mutex
        .lock()
        .map_err(|_| "acquisition cache lock poisoned".into())
}

fn data_dir() -> PathBuf {
    ProjectDirs::from("net", "fritztech", "Easel").map_or_else(
        || std::env::temp_dir().join("easel").join("data"),
        |dirs| dirs.data_dir().to_path_buf(),
    )
}

fn cache_dir() -> PathBuf {
    ProjectDirs::from("net", "fritztech", "Easel").map_or_else(
        || std::env::temp_dir().join("easel").join("cache"),
        |dirs| dirs.cache_dir().to_path_buf(),
    )
}

/// Directory that stores PNG frames decoded from imported dynamic HEIC packages.
#[must_use]
pub fn dynamic_stills_dir() -> PathBuf {
    data_dir().join("dynamic-stills")
}
