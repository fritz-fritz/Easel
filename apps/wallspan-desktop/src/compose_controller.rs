//! Compose-page presentation model, background preview rendering, and wallpaper apply.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use cxx_qt::{CxxQtThread, CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use url::Url;
use wallspan_core::{FitMode, Profile};
use wallspan_platform::{DisplayWallpaper, WallpaperOutput, select_wallpaper_backend};
use wallspan_render::{CompositionSettings, RasterJob, RenderPurpose, RenderRequest};

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
        #[qproperty(f64, zoom)]
        #[qproperty(f64, focal_x)]
        #[qproperty(f64, focal_y)]
        #[qproperty(QString, source_path)]
        #[qproperty(bool, apply_busy)]
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
    }

    impl cxx_qt::Threading for ComposeController {}
}

/// Presentation state for the Compose page.
pub struct ComposeControllerRust {
    preview_ready: bool,
    preview_status: QString,
    display_previews: QStringList,
    fit_mode_index: i32,
    zoom: f64,
    focal_x: f64,
    focal_y: f64,
    source_path: QString,
    apply_busy: bool,
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
            zoom: 1.0,
            focal_x: 0.5,
            focal_y: 0.5,
            source_path: QString::default(),
            apply_busy: false,
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

        let _ = job_tx.send(WorkerJob::Apply(ApplyJob {
            generation,
            request,
            displays,
            output_dir,
            qt_thread,
        }));
    }
}

fn build_request(
    source: &str,
    displays: Vec<wallspan_core::Display>,
    controller: Pin<&qobject::ComposeController>,
) -> RenderRequest {
    let mut profile = Profile::new("Compose");
    profile.fit_mode = fit_mode_from_index(*controller.fit_mode_index());
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
    displays: Vec<wallspan_core::Display>,
    output_dir: PathBuf,
    qt_thread: CxxQtThread<qobject::ComposeController>,
}

fn worker_sender() -> Sender<WorkerJob> {
    static SENDER: OnceLock<Sender<WorkerJob>> = OnceLock::new();
    SENDER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            thread::Builder::new()
                .name("wallspan-compose-worker".into())
                .spawn(move || worker_loop(rx))
                .expect("compose worker thread");
            tx
        })
        .clone()
}

fn worker_loop(rx: Receiver<WorkerJob>) {
    while let Ok(mut job) = rx.recv() {
        while let Ok(newer) = rx.try_recv() {
            job = newer;
        }

        match job {
            WorkerJob::Preview(preview) => run_preview(preview),
            WorkerJob::Apply(apply) => run_apply(apply),
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
    std::env::temp_dir()
        .join("wallspan")
        .join("compose-preview")
}

fn apply_cache_dir() -> PathBuf {
    std::env::temp_dir().join("wallspan").join("compose-apply")
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
