// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Compose-page presentation model, background preview rendering, and wallpaper apply.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use easel_core::{AssetId, AssetLocation, DynamicStillSet, FitMode, LayoutMode, Profile};
use easel_platform::{DisplayWallpaper, WallpaperOutput, select_wallpaper_backend};
use easel_render::{CompositionSettings, RasterJob, RenderPurpose, RenderRequest};
use url::Url;

use crate::display_session::{current_displays, current_preview_displays};

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
        #[qproperty(bool, preview_ready)]
        #[qproperty(QString, preview_status)]
        #[qproperty(QStringList, display_previews)]
        #[qproperty(i32, fit_mode_index)]
        #[qproperty(i32, layout_mode_index)]
        #[qproperty(f64, zoom)]
        #[qproperty(f64, focal_x)]
        #[qproperty(f64, focal_y)]
        #[qproperty(QString, source_path)]
        #[qproperty(bool, apply_busy)]
        #[qproperty(i32, schedule_index)]
        #[qproperty(QString, profile_name)]
        #[qproperty(i32, media_mode_index)]
        #[qproperty(QString, timeline_preview)]
        type ComposeController = super::ComposeControllerRust;

        #[qinvokable]
        #[rust_name = "set_source_path_from_url"]
        fn setSourcePathFromUrl(self: Pin<&mut Self>, url: QString);

        #[qinvokable]
        #[rust_name = "refresh_preview"]
        fn refreshPreview(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "apply_wallpaper"]
        fn applyWallpaper(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "save_profile"]
        fn saveProfile(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "preview_timeline_hour"]
        fn previewTimelineHour(self: Pin<&mut Self>, hour: f64);

        #[qinvokable]
        #[rust_name = "import_dynamic_heic_from_url"]
        fn importDynamicHeicFromUrl(self: Pin<&mut Self>, url: QString);
    }

    impl cxx_qt::Threading for ComposeController {}
}

/// Presentation state for the Compose page.
pub struct ComposeControllerRust {
    preview_ready: bool,
    preview_status: QString,
    display_previews: QStringList,
    fit_mode_index: i32,
    layout_mode_index: i32,
    zoom: f64,
    focal_x: f64,
    focal_y: f64,
    source_path: QString,
    apply_busy: bool,
    schedule_index: i32,
    profile_name: QString,
    media_mode_index: i32,
    timeline_preview: QString,
    /// Still set loaded from a HEIC import (drives timeline scrub evaluation).
    timeline_still_set: Option<DynamicStillSet>,
    job_generation: AtomicU64,
    apply_generation: AtomicU64,
    job_tx: Sender<WorkerJob>,
}

impl Default for ComposeControllerRust {
    fn default() -> Self {
        Self {
            preview_ready: false,
            preview_status: QString::from("Open a local image to render previews"),
            display_previews: QStringList::default(),
            fit_mode_index: 0,
            layout_mode_index: 0,
            zoom: 1.0,
            focal_x: 0.5,
            focal_y: 0.5,
            source_path: QString::default(),
            apply_busy: false,
            schedule_index: 0,
            profile_name: QString::from("Compose"),
            media_mode_index: 0,
            timeline_preview: QString::from(
                "Select Dynamic stills and import a HEIC, or save an hourly placeholder set.",
            ),
            timeline_still_set: None,
            job_generation: AtomicU64::new(0),
            apply_generation: AtomicU64::new(0),
            job_tx: worker_sender(),
        }
    }
}

impl qobject::ComposeController {
    fn set_source_path_from_url(mut self: Pin<&mut Self>, url: QString) {
        let path = path_from_file_url(&url.to_string());
        self.as_mut()
            .set_source_path(QString::from(path.to_string_lossy().as_ref()));
        self.as_mut()
            .set_preview_status(QString::from("Rendering preview…"));
        self.refresh_preview();
    }

    fn refresh_preview(mut self: Pin<&mut Self>) {
        let source = self.source_path().to_string();
        if source.trim().is_empty() {
            self.as_mut()
                .set_preview_status(QString::from("Open a local image to render previews"));
            self.as_mut().set_preview_ready(false);
            return;
        }

        let generation = self
            .as_mut()
            .rust_mut()
            .job_generation
            .fetch_add(1, Ordering::SeqCst)
            + 1;

        let request = build_request(&source, current_preview_displays(), self.as_ref());
        let output_dir = preview_cache_dir();
        let qt_thread = self.qt_thread();
        let job_tx = self.as_ref().rust().job_tx.clone();

        self.as_mut()
            .set_preview_status(QString::from("Rendering preview…"));

        let _ = job_tx.send(WorkerJob::Preview(PreviewJob {
            generation,
            request,
            output_dir,
            qt_thread,
        }));
    }

    fn apply_wallpaper(mut self: Pin<&mut Self>) {
        let source = self.source_path().to_string();
        if source.trim().is_empty() {
            self.as_mut()
                .set_preview_status(QString::from("Open a local image before applying"));
            return;
        }

        let generation = self
            .as_mut()
            .rust_mut()
            .apply_generation
            .fetch_add(1, Ordering::SeqCst)
            + 1;

        let displays = current_displays();
        let request = build_request(&source, displays.clone(), self.as_ref());
        let output_dir = apply_cache_dir();
        let qt_thread = self.qt_thread();
        let job_tx = self.as_ref().rust().job_tx.clone();

        self.as_mut().set_apply_busy(true);
        self.as_mut()
            .set_preview_status(QString::from("Rendering full-resolution wallpaper…"));

        if job_tx
            .send(WorkerJob::Apply(ApplyJob {
                generation,
                request,
                displays,
                output_dir,
                qt_thread,
            }))
            .is_err()
        {
            self.as_mut().set_apply_busy(false);
            self.as_mut().set_preview_status(QString::from(
                "Apply failed: background worker is unavailable",
            ));
        }
    }

    fn save_profile(mut self: Pin<&mut Self>) {
        let source = self.source_path().to_string();
        if source.trim().is_empty() {
            self.as_mut()
                .set_preview_status(QString::from("Open a local image before saving a profile"));
            return;
        }
        let name = {
            let value = self.profile_name().to_string();
            if value.trim().is_empty() {
                "Compose".to_owned()
            } else {
                value
            }
        };
        let schedule_index = *self.schedule_index();
        let media_mode_index = *self.media_mode_index();
        match crate::profile_controller::save_compose_profile(
            &name,
            &source,
            fit_mode_from_index(*self.fit_mode_index()),
            layout_mode_from_index(*self.layout_mode_index()),
            if self.zoom().is_finite() {
                (*self.zoom()).max(1.0)
            } else {
                1.0
            },
            (*self.focal_x()).clamp(0.0, 1.0),
            (*self.focal_y()).clamp(0.0, 1.0),
            schedule_index,
            media_mode_index,
        ) {
            Ok(profile) => {
                let mode = match profile.presentation {
                    easel_core::PresentationMode::DynamicStills => "dynamic stills",
                    easel_core::PresentationMode::LiveMedia => "live media",
                    easel_core::PresentationMode::Static => "static",
                };
                self.as_mut().set_preview_status(QString::from(
                    format!(
                        "Saved {} profile '{}' ({})",
                        mode,
                        profile.name,
                        profile.id.to_hyphenated_string()
                    )
                    .as_str(),
                ));
            }
            Err(error) => {
                self.as_mut().set_preview_status(QString::from(
                    format!("Save profile failed: {error}").as_str(),
                ));
            }
        }
    }

    fn preview_timeline_hour(mut self: Pin<&mut Self>, hour: f64) {
        let hour = if hour.is_finite() {
            hour.clamp(0.0, 23.99)
        } else {
            0.0
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let whole_hour = hour.floor() as u8;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let minute = ((hour.fract() * 60.0).floor() as u8).min(59);

        let still_set = self.as_ref().rust().timeline_still_set.clone();
        let (label, frame_path) = if let Some(set) = still_set.as_ref() {
            use easel_core::{DynamicEvalContext, InstantSeconds, active_frame_with_context};
            use easel_platform::system_appearance;
            // Anchor to a fixed UTC day so scrubbing only changes local wall time.
            let local_minutes = i64::from(whole_hour) * 60 + i64::from(minute);
            let now = InstantSeconds {
                unix_seconds: local_minutes * 60,
            };
            let selection = active_frame_with_context(
                set,
                DynamicEvalContext {
                    now,
                    utc_offset_minutes: 0,
                    appearance: system_appearance(),
                },
            );
            let path = resolve_library_asset_path(selection.asset_id);
            let label = format!(
                "{} ({}) · {} frames · {:?}",
                selection.key_label(),
                selection.asset_id.to_hyphenated_string(),
                set.frames.len(),
                set.schedule_kind
            );
            (label, path)
        } else {
            (
                format!(
                    "tod:{whole_hour:02}:{minute:02} (no still set loaded — import a HEIC or save Dynamic stills)"
                ),
                None,
            )
        };
        self.as_mut().set_timeline_preview(QString::from(
            format!("Simulated {whole_hour:02}:{minute:02} → {label}").as_str(),
        ));
        if let Some(path) = frame_path {
            let current = self.source_path().to_string();
            if current != path {
                self.as_mut().set_source_path(QString::from(path.as_str()));
                self.refresh_preview();
            }
        }
    }

    fn import_dynamic_heic_from_url(mut self: Pin<&mut Self>, url: QString) {
        let path = path_from_file_url(&url.to_string());
        let path_str = path.to_string_lossy().to_string();
        if path_str.trim().is_empty() {
            self.as_mut()
                .set_preview_status(QString::from("Choose a dynamic HEIC to import"));
            return;
        }
        let name = {
            let value = self.profile_name().to_string();
            if value.trim().is_empty() {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("Dynamic HEIC")
                    .to_owned()
            } else {
                value
            }
        };
        match crate::profile_controller::import_dynamic_heic_profile(
            &path_str,
            &name,
            fit_mode_from_index(*self.fit_mode_index()),
            layout_mode_from_index(*self.layout_mode_index()),
            if self.zoom().is_finite() {
                (*self.zoom()).max(1.0)
            } else {
                1.0
            },
            (*self.focal_x()).clamp(0.0, 1.0),
            (*self.focal_y()).clamp(0.0, 1.0),
        ) {
            Ok((profile, still_set, first_frame)) => {
                self.as_mut().rust_mut().timeline_still_set = Some(still_set.clone());
                self.as_mut().set_media_mode_index(1);
                self.as_mut()
                    .set_source_path(QString::from(first_frame.as_str()));
                self.as_mut()
                    .set_profile_name(QString::from(profile.name.as_str()));
                self.as_mut().set_preview_status(QString::from(
                    format!(
                        "Imported {} ({} frames, {:?}) as profile '{}'",
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("HEIC"),
                        still_set.frames.len(),
                        still_set.schedule_kind,
                        profile.name
                    )
                    .as_str(),
                ));
                self.as_mut().preview_timeline_hour(12.0);
                self.refresh_preview();
            }
            Err(error) => {
                self.as_mut().set_preview_status(QString::from(
                    format!("HEIC import failed: {error}").as_str(),
                ));
            }
        }
    }
}

fn build_request(
    source: &str,
    displays: Vec<easel_core::Display>,
    controller: Pin<&qobject::ComposeController>,
) -> RenderRequest {
    let mut profile = Profile::new("Compose");
    profile.fit_mode = fit_mode_from_index(*controller.fit_mode_index());
    profile.layout_mode = layout_mode_from_index(*controller.layout_mode_index());
    profile.zoom = if controller.zoom().is_finite() {
        (*controller.zoom()).max(1.0)
    } else {
        1.0
    };
    profile.focal_x = (*controller.focal_x()).clamp(0.0, 1.0);
    profile.focal_y = (*controller.focal_y()).clamp(0.0, 1.0);
    profile.displays = displays.iter().map(|display| display.id).collect();

    RenderRequest {
        source_path: PathBuf::from(source),
        displays,
        composition: CompositionSettings::from_profile(&profile),
        purpose: RenderPurpose::StaticWallpaper,
    }
}

enum WorkerJob {
    Preview(PreviewJob),
    Apply(ApplyJob),
}

struct PreviewJob {
    generation: u64,
    request: RenderRequest,
    output_dir: PathBuf,
    qt_thread: CxxQtThread<qobject::ComposeController>,
}

struct ApplyJob {
    generation: u64,
    request: RenderRequest,
    displays: Vec<easel_core::Display>,
    output_dir: PathBuf,
    qt_thread: CxxQtThread<qobject::ComposeController>,
}

fn worker_sender() -> Sender<WorkerJob> {
    static SENDER: OnceLock<Sender<WorkerJob>> = OnceLock::new();
    SENDER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            thread::Builder::new()
                .name("easel-compose-worker".into())
                .spawn(move || worker_loop(rx))
                .expect("compose worker thread");
            tx
        })
        .clone()
}

fn worker_loop(rx: Receiver<WorkerJob>) {
    while let Ok(first) = rx.recv() {
        // Coalesce queued work without letting Preview supersede Apply.
        // Latest Preview and latest Apply are kept independently; Apply runs
        // first so busy-state clearing cannot be skipped if Preview was newer.
        let mut preview: Option<PreviewJob> = None;
        let mut apply: Option<ApplyJob> = None;
        match first {
            WorkerJob::Preview(job) => preview = Some(job),
            WorkerJob::Apply(job) => apply = Some(job),
        }
        while let Ok(newer) = rx.try_recv() {
            match newer {
                WorkerJob::Preview(job) => preview = Some(job),
                WorkerJob::Apply(job) => apply = Some(job),
            }
        }

        if let Some(job) = apply {
            run_apply(job);
        }
        if let Some(job) = preview {
            run_preview(job);
        }
    }
}

fn run_preview(job: PreviewJob) {
    let generation = job.generation;
    let result = RasterJob {
        request: job.request,
        output_dir: job.output_dir,
    }
    .execute();

    let _ = job.qt_thread.queue(move |mut controller| {
        let current = controller
            .as_ref()
            .rust()
            .job_generation
            .load(Ordering::SeqCst);
        if current != generation {
            return;
        }

        match result {
            Ok(outputs) => {
                let mut previews = QStringList::default();
                for output in outputs {
                    let url = file_url_from_path(&output.path);
                    previews.append_clone(&QString::from(url.as_str()));
                }
                controller.as_mut().set_display_previews(previews);
                controller.as_mut().set_preview_ready(true);
                controller
                    .as_mut()
                    .set_preview_status(QString::from("Preview ready"));
            }
            Err(error) => {
                controller.as_mut().set_preview_ready(false);
                controller
                    .as_mut()
                    .set_preview_status(QString::from(format!("Preview failed: {error}").as_str()));
            }
        }
    });
}

fn run_apply(job: ApplyJob) {
    let generation = job.generation;
    let apply_result: Result<(), String> = (|| {
        let outputs = RasterJob {
            request: job.request,
            output_dir: job.output_dir,
        }
        .execute()
        .map_err(|error| error.to_string())?;

        let mut wallpapers = Vec::with_capacity(outputs.len());
        for output in outputs {
            let logical_rect = job
                .displays
                .iter()
                .find(|display| display.id == output.display_id)
                .map(|display| display.logical_rect)
                .ok_or_else(|| "display missing for raster output".to_owned())?;
            wallpapers.push(DisplayWallpaper {
                display_id: output.display_id,
                path: output.path,
                logical_rect,
            });
        }

        let backend = select_wallpaper_backend().map_err(|error| error.to_string())?;
        backend
            .apply(&WallpaperOutput::PerDisplay(wallpapers))
            .map_err(|error| error.to_string())?;
        Ok(())
    })();

    let _ = job.qt_thread.queue(move |mut controller| {
        let current = controller
            .as_ref()
            .rust()
            .apply_generation
            .load(Ordering::SeqCst);
        if current != generation {
            return;
        }

        controller.as_mut().set_apply_busy(false);
        match apply_result {
            Ok(()) => {
                controller
                    .as_mut()
                    .set_preview_status(QString::from("Wallpaper applied"));
            }
            Err(error) => {
                controller
                    .as_mut()
                    .set_preview_status(QString::from(format!("Apply failed: {error}").as_str()));
            }
        }
    });
}

fn fit_mode_from_index(index: i32) -> FitMode {
    match index {
        1 => FitMode::Contain,
        2 => FitMode::Stretch,
        3 => FitMode::Native,
        _ => FitMode::Cover,
    }
}

fn layout_mode_from_index(index: i32) -> LayoutMode {
    match index {
        1 => LayoutMode::Digital,
        _ => LayoutMode::PhysicalSpan,
    }
}

/// Decodes a `file://` URL (or plain path) into a filesystem path.
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

/// Builds a `file://` URL suitable for QML `Image` sources on all platforms.
fn file_url_from_path(path: &Path) -> String {
    Url::from_file_path(path).map_or_else(|()| path.to_string_lossy().into_owned(), String::from)
}

fn preview_cache_dir() -> PathBuf {
    std::env::temp_dir().join("easel").join("compose-preview")
}

fn apply_cache_dir() -> PathBuf {
    std::env::temp_dir().join("easel").join("compose-apply")
}

fn resolve_library_asset_path(asset_id: AssetId) -> Option<String> {
    use crate::library_session::library_store;

    let library = library_store().ok()?;
    let asset = library.get_asset(asset_id).ok()??;
    match asset.location {
        AssetLocation::Local { path } => Some(path),
        AssetLocation::Remote { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_from_file_url_percent_decodes() {
        let path = path_from_file_url("file:///tmp/my%20wallpapers/a%2Bb.png");
        assert_eq!(path, PathBuf::from("/tmp/my wallpapers/a+b.png"));
    }

    #[test]
    fn file_url_from_path_round_trips_spaces() {
        let original = PathBuf::from("/tmp/my wallpapers/preview.png");
        let url = file_url_from_path(&original);
        assert!(url.starts_with("file:"));
        assert!(url.contains("%20") || url.contains(' '));
        let restored = path_from_file_url(&url);
        assert_eq!(restored, original);
    }
}
