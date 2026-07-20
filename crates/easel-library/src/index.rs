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
                match self.index_file(&path)? {
                    IndexOutcome::Created | IndexOutcome::Updated => count += 1,
                    IndexOutcome::Skipped => {}
                }
            }
        }
        Ok(count)
    }

    /// Indexes or refreshes one still-image file.
    pub fn index_file(&self, path: &Path) -> Result<IndexOutcome, IndexError> {
        let canonical = fs::canonicalize(path).map_err(|error| IndexError::Io {
            path: path.to_path_buf(),
            source: error,
        })?;
        let path_string = canonical.to_string_lossy().into_owned();
        let Some(dimensions) = probe_dimensions(&canonical) else {
            return Ok(IndexOutcome::Skipped);
        };
        let title = canonical
            .file_stem()
            .map(|value| value.to_string_lossy().into_owned());

        if let Some(existing) = self.store.find_by_path(&path_string)? {
            let mut refreshed = existing;
            refreshed.title = title.or(refreshed.title);
            refreshed.media = MediaMetadata::StillImage { dimensions };
            refreshed.retrieved_at_unix = Some(now_unix());
            self.store.upsert_asset(&refreshed)?;
            return Ok(IndexOutcome::Updated);
        }

        let asset = MediaAsset {
            id: AssetId::new(),
            provider_id: None,
            title,
            media: MediaMetadata::StillImage { dimensions },
            location: AssetLocation::Local { path: path_string },
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
        Ok(IndexOutcome::Created)
    }
}

/// Result of indexing one path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexOutcome {
    /// A new library row was inserted.
    Created,
    /// An existing row was refreshed (dimensions / timestamps).
    Updated,
    /// The file could not be probed and was left unchanged.
    Skipped,
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
        .map_or(0, |duration| duration.as_secs())
}

fn probe_dimensions(path: &Path) -> Option<MediaDimensions> {
    let (width, height) = image::image_dimensions(path).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some(MediaDimensions { width, height })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};
    use uuid::Uuid;

    #[test]
    fn indexes_still_images_in_folder() {
        let root = std::env::temp_dir().join(format!("easel-idx-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        RgbImage::from_pixel(64, 48, Rgb([10, 20, 30]))
            .save(root.join("a.png"))
            .unwrap();
        fs::write(root.join("notes.txt"), b"skip").unwrap();
        let store = LibraryStore::open(root.join("library.db")).unwrap();
        let indexer = LocalIndexer::new(&store);
        let count = indexer.add_and_scan(&root, false).unwrap();
        assert_eq!(count, 1);
        let assets = store.list_assets(10).unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].media.dimensions().width, 64);
        assert_eq!(assets[0].media.dimensions().height, 48);
        assert_eq!(store.list_folders().unwrap().len(), 1);
    }

    #[test]
    fn refresh_updates_dimensions_for_existing_path() {
        let root = std::env::temp_dir().join(format!("easel-idx-refresh-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("a.png");
        RgbImage::from_pixel(32, 32, Rgb([1, 2, 3]))
            .save(&path)
            .unwrap();
        let store = LibraryStore::open(root.join("library.db")).unwrap();
        let indexer = LocalIndexer::new(&store);
        assert_eq!(indexer.index_file(&path).unwrap(), IndexOutcome::Created);
        RgbImage::from_pixel(80, 60, Rgb([4, 5, 6]))
            .save(&path)
            .unwrap();
        assert_eq!(indexer.index_file(&path).unwrap(), IndexOutcome::Updated);
        let asset = store
            .find_by_path(&fs::canonicalize(&path).unwrap().to_string_lossy())
            .unwrap()
            .unwrap();
        assert_eq!(asset.media.dimensions().width, 80);
        assert_eq!(asset.media.dimensions().height, 60);
    }
}
