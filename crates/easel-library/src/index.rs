// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Local still-image folder indexing.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use easel_core::{
    AssetId, AssetLocation, HistoryAction, HistoryEvent, MediaAsset, MediaDimensions, MediaMetadata,
};
use thiserror::Error;

use crate::store::{LibraryStore, LibraryStoreError};

/// Supported still-image file extensions for Stage 3 indexing.
const STILL_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"];

/// Returns whether `extension` is a still-image type indexed by Easel.
#[must_use]
pub fn still_image_extension(extension: &str) -> bool {
    let lower = extension.to_ascii_lowercase();
    STILL_EXTENSIONS.iter().any(|candidate| *candidate == lower)
}

/// A registered library folder root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexedFolder {
    /// Absolute folder path.
    pub path: PathBuf,
    /// Whether subdirectories are scanned.
    pub recursive: bool,
}

/// Scans folders and upserts still-image assets into the library store.
pub struct LocalIndexer<'a> {
    store: &'a LibraryStore,
}

impl<'a> LocalIndexer<'a> {
    /// Creates an indexer bound to `store`.
    #[must_use]
    pub fn new(store: &'a LibraryStore) -> Self {
        Self { store }
    }

    /// Registers `folder` and indexes matching still images.
    pub fn add_and_scan(&self, folder: &Path, recursive: bool) -> Result<usize, IndexError> {
        let canonical = fs::canonicalize(folder).map_err(|error| IndexError::Io {
            path: folder.to_path_buf(),
            source: error,
        })?;
        if !canonical.is_dir() {
            return Err(IndexError::NotDirectory(canonical));
        }
        self.store
            .add_folder(&canonical.to_string_lossy(), recursive)?;
        self.scan_path(&canonical, recursive)
    }

    /// Re-indexes every registered folder.
    pub fn rescan_all(&self) -> Result<usize, IndexError> {
        let mut total = 0;
        for (path, recursive) in self.store.list_folders()? {
            total += self.scan_path(Path::new(&path), recursive)?;
        }
        Ok(total)
    }

    /// Indexes still images under `folder`.
    pub fn scan_path(&self, folder: &Path, recursive: bool) -> Result<usize, IndexError> {
        let mut count = 0;
        let mut stack = vec![folder.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let entries = fs::read_dir(&dir).map_err(|error| IndexError::Io {
                path: dir.clone(),
                source: error,
            })?;
            for entry in entries {
                let entry = entry.map_err(|error| IndexError::Io {
                    path: dir.clone(),
                    source: error,
                })?;
                let path = entry.path();
                let file_type = entry.file_type().map_err(|error| IndexError::Io {
                    path: path.clone(),
                    source: error,
                })?;
                if file_type.is_dir() {
                    if recursive {
                        stack.push(path);
                    }
                    continue;
                }
                if !file_type.is_file() {
                    continue;
                }
                let extension = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default();
                if !still_image_extension(extension) {
                    continue;
                }
                if self.index_file(&path)? {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// Indexes one still-image file, returning whether a new record was created.
    pub fn index_file(&self, path: &Path) -> Result<bool, IndexError> {
        let canonical = fs::canonicalize(path).map_err(|error| IndexError::Io {
            path: path.to_path_buf(),
            source: error,
        })?;
        let path_string = canonical.to_string_lossy().into_owned();
        if self.store.find_by_path(&path_string)?.is_some() {
            return Ok(false);
        }

        let dimensions = probe_dimensions(&canonical).unwrap_or(MediaDimensions {
            width: 1,
            height: 1,
        });
        let title = canonical
            .file_stem()
            .map(|value| value.to_string_lossy().into_owned());
        let asset = MediaAsset {
            id: AssetId::new(),
            provider_id: None,
            title,
            media: MediaMetadata::StillImage { dimensions },
            location: AssetLocation::Local {
                path: path_string.clone(),
            },
            license: None,
            attribution: None,
            content_safety: easel_core::ContentSafety::Safe,
            source: Some("local".into()),
            use_reporting_url: None,
            retrieved_at_unix: Some(now_unix()),
        };
        self.store.upsert_asset(&asset)?;
        self.store.record_history(&HistoryEvent::new(
            asset.id,
            HistoryAction::Discovered,
            now_unix(),
        ))?;
        Ok(true)
    }
}

/// Folder indexing failure.
#[derive(Debug, Error)]
pub enum IndexError {
    /// Path exists but is not a directory.
    #[error("not a directory: {0}")]
    NotDirectory(PathBuf),
    /// Filesystem error while scanning.
    #[error("io error for {path}: {source}")]
    Io {
        /// Path associated with the failure.
        path: PathBuf,
        /// Underlying IO error.
        source: std::io::Error,
    },
    /// Library store failure.
    #[error(transparent)]
    Store(#[from] LibraryStoreError),
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn probe_dimensions(path: &Path) -> Option<MediaDimensions> {
    // Keep the library crate free of the image decoder stack; Stage 3 records a
    // placeholder until Compose/decode fills accurate dimensions on use.
    let _ = path;
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn indexes_still_images_in_folder() {
        let root = std::env::temp_dir().join(format!("easel-idx-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.png"), b"not-a-real-png").unwrap();
        fs::write(root.join("notes.txt"), b"skip").unwrap();
        let store = LibraryStore::open(root.join("library.db")).unwrap();
        let indexer = LocalIndexer::new(&store);
        let count = indexer.add_and_scan(&root, false).unwrap();
        assert_eq!(count, 1);
        assert_eq!(store.list_assets(10).unwrap().len(), 1);
        assert_eq!(store.list_folders().unwrap().len(), 1);
    }
}
