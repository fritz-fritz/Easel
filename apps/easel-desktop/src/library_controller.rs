// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Library-page folder index and favorites presentation model.

use std::fs;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use easel_core::{
    AssetLocation, MediaAsset, MediaDimensions, MediaMetadata, PixelBudget, assess_suitability,
};
use easel_library::{
    FolderWatchEvent, FolderWatcher, LocalIndexer, poster_path_for_asset, video_extension,
};
use serde_json::json;
use url::Url;

use crate::display_session::current_displays;
use crate::library_session::{library_store, posters_dir};

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
        #[qproperty(QStringList, video_probe_queue)]
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

        #[qinvokable]
        #[rust_name = "video_probe_temp_path"]
        fn videoProbeTempPath(self: &Self, path: QString) -> QString;

        #[qinvokable]
        #[rust_name = "complete_video_probe"]
        fn completeVideoProbe(
            self: Pin<&mut Self>,
            path: QString,
            probe_json: QString,
            poster_path: QString,
        );

        #[qinvokable]
        #[rust_name = "skip_video_probe"]
        fn skipVideoProbe(self: Pin<&mut Self>, path: QString, error: QString);
    }
}

/// Presentation state for the Library page.
pub struct LibraryControllerRust {
    status_text: QString,
    folder_model: QStringList,
    asset_model: QStringList,
    favorite_model: QStringList,
    selected_file_url: QString,
    video_probe_queue: QStringList,
    assets: Vec<MediaAsset>,
    favorites: Vec<MediaAsset>,
    watcher: Option<FolderWatcher>,
    pending_videos: Vec<PathBuf>,
}

impl Default for LibraryControllerRust {
    fn default() -> Self {
        let mut controller = Self {
            status_text: QString::from("Add a local folder to index images and motion media."),
            folder_model: QStringList::default(),
            asset_model: QStringList::default(),
            favorite_model: QStringList::default(),
            selected_file_url: QString::default(),
            video_probe_queue: QStringList::default(),
            assets: Vec::new(),
            favorites: Vec::new(),
            watcher: None,
            pending_videos: Vec::new(),
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
        self.asset_model = asset_model_list(&self.assets, budget, &posters_dir());
        self.favorite_model = asset_model_list(&self.favorites, budget, &posters_dir());
        let pending = self.pending_videos.len();
        self.status_text = QString::from(
            format!(
                "{folder_count} folder(s), {asset_count} indexed asset(s), {favorite_count} favorite(s){}",
                if pending > 0 {
                    format!(", probing {pending} video(s)")
                } else {
                    String::new()
                }
            )
            .as_str(),
        );
        Ok(())
    }

    fn publish_probe_queue(&mut self) {
        self.video_probe_queue = qstring_list(self.pending_videos.iter().map(|path| {
            Url::from_file_path(path).map_or_else(
                |()| path.to_string_lossy().into_owned(),
                |url| url.to_string(),
            )
        }));
    }

    fn enqueue_videos(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            if self.pending_videos.iter().any(|existing| existing == &path) {
                continue;
            }
            self.pending_videos.push(path);
        }
        self.publish_probe_queue();
    }

    fn pop_pending(&mut self, path: &Path) {
        self.pending_videos.retain(|existing| existing != path);
        self.publish_probe_queue();
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
                let queue = self.as_ref().rust().video_probe_queue.clone();
                self.as_mut().set_status_text(status);
                self.as_mut().set_folder_model(folders);
                self.as_mut().set_asset_model(assets);
                self.as_mut().set_favorite_model(favorites);
                self.as_mut().set_video_probe_queue(queue);
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
            let posters = posters_dir();
            let indexer = LocalIndexer::new(&store).with_posters_dir(&posters);
            let count = indexer
                .add_and_scan(&path, true)
                .map_err(|error| error.to_string())?;
            let videos = LocalIndexer::collect_video_paths(&path, true)
                .map_err(|error| error.to_string())?;
            Ok::<(usize, Vec<PathBuf>), String>((count, videos))
        })();
        match result {
            Ok((count, videos)) => {
                let video_count = videos.len();
                self.as_mut().rust_mut().enqueue_videos(videos);
                let queue = self.as_ref().rust().video_probe_queue.clone();
                self.as_mut().set_video_probe_queue(queue);
                self.as_mut().set_status_text(QString::from(
                    format!(
                        "Indexed {count} still/GIF file(s) from {}; queued {video_count} video(s) for Qt probe",
                        path.display()
                    )
                    .as_str(),
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
            let posters = posters_dir();
            let indexer = LocalIndexer::new(&store).with_posters_dir(&posters);
            let count = indexer.rescan_all().map_err(|error| error.to_string())?;
            let mut videos = Vec::new();
            for (folder, recursive) in store.list_folders().map_err(|error| error.to_string())? {
                videos.extend(
                    LocalIndexer::collect_video_paths(Path::new(&folder), recursive)
                        .map_err(|error| error.to_string())?,
                );
            }
            Ok::<(usize, Vec<PathBuf>), String>((count, videos))
        })();
        match result {
            Ok((count, videos)) => {
                let video_count = videos.len();
                self.as_mut().rust_mut().enqueue_videos(videos);
                let queue = self.as_ref().rust().video_probe_queue.clone();
                self.as_mut().set_video_probe_queue(queue);
                self.as_mut().set_status_text(QString::from(
                    format!(
                        "Rescan complete; {count} still/GIF file(s), queued {video_count} video(s)"
                    )
                    .as_str(),
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
                let label = if asset.media.requires_live_surface() {
                    "Opening motion media in Compose"
                } else {
                    "Opening local image in Compose"
                };
                self.as_mut().set_status_text(QString::from(label));
            }
            Err(()) => {
                self.as_mut()
                    .set_status_text(QString::from("Could not build file URL"));
            }
        }
    }

    fn poll_watch(mut self: Pin<&mut Self>) {
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
        let mut videos = Vec::new();
        let result = (|| {
            let store = library_store()?;
            let posters = posters_dir();
            let indexer = LocalIndexer::new(&store).with_posters_dir(&posters);
            for event in events {
                match event {
                    FolderWatchEvent::Upsert(path) => {
                        let extension = path
                            .extension()
                            .and_then(|value| value.to_str())
                            .unwrap_or_default();
                        if video_extension(extension) {
                            videos.push(path);
                        } else {
                            let _ = indexer.index_file(&path);
                        }
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
            if !videos.is_empty() {
                self.as_mut().rust_mut().enqueue_videos(videos);
                let queue = self.as_ref().rust().video_probe_queue.clone();
                self.as_mut().set_video_probe_queue(queue);
            }
            self.refresh();
        }
    }

    #[allow(clippy::unused_self)]
    fn video_probe_temp_path(&self, path: QString) -> QString {
        let source = PathBuf::from(path.to_string());
        let stem = source.file_stem().map_or_else(
            || "video".into(),
            |value| value.to_string_lossy().into_owned(),
        );
        let dest = std::env::temp_dir().join(format!(
            "easel-video-poster-{}-{}.png",
            std::process::id(),
            stem
        ));
        QString::from(dest.to_string_lossy().as_ref())
    }

    fn complete_video_probe(
        mut self: Pin<&mut Self>,
        path: QString,
        probe_json: QString,
        poster_path: QString,
    ) {
        let path_buf = PathBuf::from(path.to_string());
        let Ok(probe) = serde_json::from_str::<VideoProbePayload>(&probe_json.to_string()) else {
            self.skip_video_probe(path, QString::from("Invalid video probe payload"));
            return;
        };
        if probe.width == 0 || probe.height == 0 {
            self.skip_video_probe(path, QString::from("Decoder reported empty resolution"));
            return;
        }
        let media = MediaMetadata::Video {
            dimensions: MediaDimensions {
                width: probe.width,
                height: probe.height,
            },
            duration_ms: probe.duration_ms.filter(|value| *value > 0),
            frame_rate: None,
            container: non_empty_owned(probe.container),
            video_codec: non_empty_owned(probe.video_codec),
            has_audio: probe.has_audio,
        };
        let poster = {
            let value = poster_path.to_string();
            if value.trim().is_empty() {
                None
            } else {
                Some(PathBuf::from(value))
            }
        };
        let result = (|| {
            let store = library_store()?;
            let posters = posters_dir();
            let indexer = LocalIndexer::new(&store).with_posters_dir(&posters);
            indexer
                .index_with_metadata(&path_buf, media, poster.as_deref())
                .map_err(|error| error.to_string())
        })();
        self.as_mut().rust_mut().pop_pending(&path_buf);
        let queue = self.as_ref().rust().video_probe_queue.clone();
        self.as_mut().set_video_probe_queue(queue);
        if let Err(error) = result {
            self.as_mut().set_status_text(QString::from(
                format!("Video probe index failed: {error}").as_str(),
            ));
        }
        if let Some(temp) = poster {
            let _ = fs::remove_file(temp);
        }
        self.refresh();
    }

    fn skip_video_probe(mut self: Pin<&mut Self>, path: QString, error: QString) {
        let path_buf = PathBuf::from(path.to_string());
        self.as_mut().rust_mut().pop_pending(&path_buf);
        let queue = self.as_ref().rust().video_probe_queue.clone();
        self.as_mut().set_video_probe_queue(queue);
        self.as_mut().set_status_text(QString::from(
            format!("Skipped video {}: {error}", path_buf.display()).as_str(),
        ));
        self.refresh();
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoProbePayload {
    width: u32,
    height: u32,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    container: Option<String>,
    #[serde(default)]
    video_codec: Option<String>,
    #[serde(default)]
    has_audio: bool,
}

fn non_empty_owned(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

fn asset_model_list(assets: &[MediaAsset], budget: PixelBudget, posters_dir: &Path) -> QStringList {
    let mut list = QStringList::default();
    for asset in assets {
        let assessment = assess_suitability(asset.media.dimensions(), budget);
        let preview = match &asset.location {
            AssetLocation::Local { path } => {
                if asset.media.requires_live_surface() {
                    let poster = poster_path_for_asset(posters_dir, asset.id);
                    if poster.is_file() {
                        Url::from_file_path(&poster)
                            .map_or_else(|()| poster.display().to_string(), |url| url.to_string())
                    } else {
                        Url::from_file_path(Path::new(path))
                            .map_or_else(|()| path.clone(), |url| url.as_str().to_owned())
                    }
                } else {
                    Url::from_file_path(Path::new(path))
                        .map_or_else(|()| path.clone(), |url| url.as_str().to_owned())
                }
            }
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
    if let Ok(url) = Url::parse(trimmed)
        && url.scheme() == "file"
        && let Ok(path) = url.to_file_path()
    {
        return path;
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
    } else if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
        && let Ok(parent_canonical) = fs::canonicalize(parent)
    {
        push_unique(
            &mut candidates,
            parent_canonical.join(name).to_string_lossy().into_owned(),
        );
    }
    candidates
}
