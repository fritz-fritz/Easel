// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Library-page folder index and favorites presentation model.

use std::fs;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use easel_core::{AssetLocation, MediaAsset, PixelBudget, assess_suitability};
use easel_library::{FolderWatchEvent, FolderWatcher, LocalIndexer};
use serde_json::json;
use url::Url;

use crate::display_session::current_displays;
use crate::library_session::library_store;

#[cxx_qt::bridge]
mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status_text)]
        #[qproperty(QStringList, folder_model)]
        #[qproperty(QStringList, asset_model)]
        #[qproperty(QStringList, favorite_model)]
        #[qproperty(QString, selected_file_url)]
        type LibraryController = super::LibraryControllerRust;

        #[qinvokable]
        #[rust_name = "refresh"]
        fn refresh(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "add_folder_from_url"]
        fn addFolderFromUrl(self: Pin<&mut Self>, url: QString);

        #[qinvokable]
        #[rust_name = "rescan"]
        fn rescan(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "use_asset"]
        fn useAsset(self: Pin<&mut Self>, index: i32);

        #[qinvokable]
        #[rust_name = "poll_watch"]
        fn pollWatch(self: Pin<&mut Self>);
    }
}

/// Presentation state for the Library page.
pub struct LibraryControllerRust {
    status_text: QString,
    folder_model: QStringList,
    asset_model: QStringList,
    favorite_model: QStringList,
    selected_file_url: QString,
    assets: Vec<MediaAsset>,
    favorites: Vec<MediaAsset>,
    watcher: Option<FolderWatcher>,
}

impl Default for LibraryControllerRust {
    fn default() -> Self {
        let mut controller = Self {
            status_text: QString::from("Add a local folder to index still images."),
            folder_model: QStringList::default(),
            asset_model: QStringList::default(),
            favorite_model: QStringList::default(),
            selected_file_url: QString::default(),
            assets: Vec::new(),
            favorites: Vec::new(),
            watcher: None,
        };
        let _ = controller.reload_models();
        controller
    }
}

impl LibraryControllerRust {
    fn reload_models(&mut self) -> Result<(), String> {
        let store = library_store()?;
        let folders = store.list_folders().map_err(|error| error.to_string())?;
        let assets = store.list_assets(48).map_err(|error| error.to_string())?;
        let favorites = store
            .list_favorites(48)
            .map_err(|error| error.to_string())?;

        let folder_paths: Vec<PathBuf> = folders
            .iter()
            .map(|(path, _)| PathBuf::from(path))
            .collect();
        self.watcher = FolderWatcher::start(&folder_paths).ok();

        self.folder_model = qstring_list(folders.into_iter().map(|(path, recursive)| {
            if recursive {
                format!("{path} (recursive)")
            } else {
                path
            }
        }));
        let budget = PixelBudget::from_displays(&current_displays());
        let folder_count = folder_paths.len();
        let asset_count = assets.len();
        let favorite_count = favorites.len();
        self.assets = assets;
        self.favorites = favorites;
        self.asset_model = asset_model_list(&self.assets, budget);
        self.favorite_model = asset_model_list(&self.favorites, budget);
        self.status_text = QString::from(
            format!(
                "{folder_count} folder(s), {asset_count} indexed asset(s), {favorite_count} favorite(s)"
            )
            .as_str(),
        );
        Ok(())
    }
}

impl qobject::LibraryController {
    fn refresh(mut self: Pin<&mut Self>) {
        match self.as_mut().rust_mut().reload_models() {
            Ok(()) => {
                let status = self.as_ref().rust().status_text.clone();
                let folders = self.as_ref().rust().folder_model.clone();
                let assets = self.as_ref().rust().asset_model.clone();
                let favorites = self.as_ref().rust().favorite_model.clone();
                self.as_mut().set_status_text(status);
                self.as_mut().set_folder_model(folders);
                self.as_mut().set_asset_model(assets);
                self.as_mut().set_favorite_model(favorites);
            }
            Err(error) => {
                self.as_mut().set_status_text(QString::from(error.as_str()));
            }
        }
    }

    fn add_folder_from_url(mut self: Pin<&mut Self>, url: QString) {
        let path = path_from_file_url(&url.to_string());
        if path.as_os_str().is_empty() {
            self.as_mut()
                .set_status_text(QString::from("Choose a folder to index"));
            return;
        }
        let result = (|| {
            let store = library_store()?;
            let indexer = LocalIndexer::new(&store);
            let count = indexer
                .add_and_scan(&path, true)
                .map_err(|error| error.to_string())?;
            Ok::<usize, String>(count)
        })();
        match result {
            Ok(count) => {
                self.as_mut().set_status_text(QString::from(
                    format!("Indexed {count} new still image(s) from {}", path.display()).as_str(),
                ));
                self.refresh();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(QString::from(format!("Index failed: {error}").as_str()));
            }
        }
    }

    fn rescan(mut self: Pin<&mut Self>) {
        let result = (|| {
            let store = library_store()?;
            let indexer = LocalIndexer::new(&store);
            indexer.rescan_all().map_err(|error| error.to_string())
        })();
        match result {
            Ok(count) => {
                self.as_mut().set_status_text(QString::from(
                    format!("Rescan complete; {count} new still image(s)").as_str(),
                ));
                self.refresh();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(QString::from(format!("Rescan failed: {error}").as_str()));
            }
        }
    }

    fn use_asset(mut self: Pin<&mut Self>, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(asset) = self.as_ref().rust().assets.get(index).cloned() else {
            return;
        };
        let path = match &asset.location {
            AssetLocation::Local { path } => PathBuf::from(path),
            AssetLocation::Remote { .. } => {
                self.as_mut().set_status_text(QString::from(
                    "Remote favorites open from Discover after download",
                ));
                return;
            }
        };
        match Url::from_file_path(&path) {
            Ok(url) => {
                self.as_mut()
                    .set_selected_file_url(QString::from(url.as_str()));
                self.as_mut()
                    .set_status_text(QString::from("Opening local image in Compose"));
            }
            Err(()) => {
                self.as_mut()
                    .set_status_text(QString::from("Could not build file URL"));
            }
        }
    }

    fn poll_watch(self: Pin<&mut Self>) {
        let events = self
            .as_ref()
            .rust()
            .watcher
            .as_ref()
            .map(FolderWatcher::drain)
            .unwrap_or_default();
        if events.is_empty() {
            return;
        }
        let result = (|| {
            let store = library_store()?;
            let indexer = LocalIndexer::new(&store);
            for event in events {
                match event {
                    FolderWatchEvent::Upsert(path) => {
                        let _ = indexer.index_file(&path);
                    }
                    FolderWatchEvent::Remove(path) => {
                        for candidate in removal_path_candidates(&path) {
                            let _ = store.remove_by_path(&candidate);
                        }
                    }
                }
            }
            Ok::<(), String>(())
        })();
        if result.is_ok() {
            self.refresh();
        }
    }
}

fn asset_model_list(assets: &[MediaAsset], budget: PixelBudget) -> QStringList {
    let mut list = QStringList::default();
    for asset in assets {
        let assessment = assess_suitability(asset.media.dimensions(), budget);
        let preview = match &asset.location {
            AssetLocation::Local { path } => Url::from_file_path(Path::new(path))
                .map_or_else(|()| path.clone(), |url| url.as_str().to_owned()),
            AssetLocation::Remote { preview_url, .. } => preview_url.as_str().to_owned(),
        };
        let creator = asset
            .attribution
            .as_ref()
            .map_or_else(|| "Local".into(), |value| value.creator_name.clone());
        let license = asset
            .license
            .as_ref()
            .map_or_else(|| "local".into(), |value| value.identifier.clone());
        let source = asset.source.clone().unwrap_or_else(|| "local".into());
        let payload = json!({
            "title": asset.title.clone().unwrap_or_else(|| "Untitled".into()),
            "creator": creator,
            "license": license,
            "preview": preview,
            "score": assessment.score,
            "meetsMinimum": assessment.meets_minimum,
            "source": source,
        });
        list.append_clone(&QString::from(payload.to_string().as_str()));
    }
    list
}

fn qstring_list(values: impl IntoIterator<Item = String>) -> QStringList {
    let mut list = QStringList::default();
    for value in values {
        list.append_clone(&QString::from(value.as_str()));
    }
    list
}

fn path_from_file_url(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return PathBuf::new();
    }
    if let Ok(url) = Url::parse(trimmed) {
        if url.scheme() == "file" {
            if let Ok(path) = url.to_file_path() {
                return path;
            }
        }
    }
    PathBuf::from(trimmed)
}

/// Builds path strings that may match a previously indexed canonical local asset.
fn removal_path_candidates(path: &Path) -> Vec<String> {
    let mut candidates = Vec::new();
    let push_unique = |list: &mut Vec<String>, value: String| {
        if !list.iter().any(|existing| existing == &value) {
            list.push(value);
        }
    };

    push_unique(&mut candidates, path.to_string_lossy().into_owned());
    if let Ok(canonical) = fs::canonicalize(path) {
        push_unique(&mut candidates, canonical.to_string_lossy().into_owned());
    } else if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if let Ok(parent_canonical) = fs::canonicalize(parent) {
            push_unique(
                &mut candidates,
                parent_canonical.join(name).to_string_lossy().into_owned(),
            );
        }
    }
    candidates
}
