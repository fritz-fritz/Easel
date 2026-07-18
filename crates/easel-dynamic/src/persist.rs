// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Persist imported dynamic desktops into local PNG assets + a still set shell.

use std::path::{Path, PathBuf};

use easel_core::{
    AssetId, AssetLocation, ContentSafety, DynamicStillSet, MediaAsset, MediaDimensions,
    MediaMetadata, ProfileId,
};
use easel_render::atomic_write_png;
use thiserror::Error;

use crate::heic::{HeicImportError, ImportedDynamicDesktop};

/// Result of writing decoded HEIC frames into a library asset directory.
#[derive(Clone, Debug)]
pub struct PersistedDynamicImport {
    /// Still-set template (validated) ready for `AutomationStore::upsert_still_set`.
    pub still_set: DynamicStillSet,
    /// Library assets corresponding to each frame (same order as `still_set.frames`).
    pub assets: Vec<MediaAsset>,
    /// Absolute paths of written PNG frames.
    pub frame_paths: Vec<PathBuf>,
}

/// Writes each decoded frame as a PNG under `asset_dir` and builds library assets + still set.
pub fn persist_imported_desktop(
    imported: &ImportedDynamicDesktop,
    name: impl Into<String>,
    profile_id: ProfileId,
    asset_dir: impl AsRef<Path>,
) -> Result<PersistedDynamicImport, PersistError> {
    let name = name.into();
    let asset_dir = asset_dir.as_ref();
    std::fs::create_dir_all(asset_dir)?;

    let mut assets = Vec::with_capacity(imported.frames.len());
    let mut asset_ids = Vec::with_capacity(imported.frames.len());
    let mut frame_paths = Vec::with_capacity(imported.frames.len());

    for frame in &imported.frames {
        let asset_id = AssetId::new();
        let file_name = format!(
            "{}-frame-{:02}.png",
            asset_id.to_hyphenated_string(),
            frame.index
        );
        let path = asset_dir.join(file_name);
        atomic_write_png(&path, &frame.image)?;
        let asset = MediaAsset {
            id: asset_id,
            provider_id: None,
            title: Some(format!("{name} · {}", frame.key.label())),
            media: MediaMetadata::StillImage {
                dimensions: MediaDimensions {
                    width: frame.image.width(),
                    height: frame.image.height(),
                },
            },
            location: AssetLocation::Local {
                path: path.display().to_string(),
            },
            license: None,
            attribution: None,
            content_safety: ContentSafety::Safe,
            source: Some("dynamic-heic".into()),
            use_reporting_url: None,
            retrieved_at_unix: None,
        };
        asset_ids.push(asset_id);
        assets.push(asset);
        frame_paths.push(path);
    }

    let still_set = imported.into_still_set_template(name, profile_id, &asset_ids)?;
    Ok(PersistedDynamicImport {
        still_set,
        assets,
        frame_paths,
    })
}

/// Persist failures.
#[derive(Debug, Error)]
pub enum PersistError {
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// PNG write failure.
    #[error(transparent)]
    Raster(#[from] easel_render::RasterError),
    /// Domain / HEIC template failure.
    #[error(transparent)]
    Import(#[from] HeicImportError),
}
