//! Compose-page presentation model and background preview rendering.

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
use wallspan_render::{CompositionSettings, RasterJob, RenderPurpose, RenderRequest};

use crate::fixtures::{dev_displays, preview_displays};

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
        type ComposeController = super::ComposeControllerRust;

        #[qinvokable]
        #[rust_name = "set_source_path_from_url"]
        fn setSourcePathFromUrl(self: Pin<&mut Self>, url: QString);

        #[qinvokable]
        #[rust_name = "refresh_preview"]
        fn refreshPreview(self: Pin<&mut Self>);
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
    job_generation: AtomicU64,
    job_tx: Sender<PreviewJob>,
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
            job_generation: AtomicU64::new(0),
            job_tx: preview_job_sender(),
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

        let mut profile = Profile::new("Compose preview");
        profile.fit_mode = fit_mode_from_index(*self.fit_mode_index());
        profile.zoom = if self.zoom().is_finite() {
            (*self.zoom()).max(1.0)
        } else {
            1.0
        };
        profile.focal_x = (*self.focal_x()).clamp(0.0, 1.0);
        profile.focal_y = (*self.focal_y()).clamp(0.0, 1.0);
        profile.displays = dev_displays()
            .into_iter()
            .map(|display| display.id)
            .collect();

        let request = RenderRequest {
            source_path: PathBuf::from(source),
            displays: preview_displays(),
            composition: CompositionSettings::from_profile(&profile),
            purpose: RenderPurpose::StaticWallpaper,
        };
        let output_dir = preview_cache_dir();
        let qt_thread = self.qt_thread();
        let job_tx = self.as_ref().rust().job_tx.clone();

        self.as_mut()
            .set_preview_status(QString::from("Rendering preview…"));

        // Latest-wins: the worker drains queued jobs so drag updates do not spawn threads.
        let _ = job_tx.send(PreviewJob {
            generation,
            request,
            output_dir,
            qt_thread,
        });
    }
}

struct PreviewJob {
    generation: u64,
    request: RenderRequest,
    output_dir: PathBuf,
    qt_thread: CxxQtThread<qobject::ComposeController>,
}

fn preview_job_sender() -> Sender<PreviewJob> {
    static SENDER: OnceLock<Sender<PreviewJob>> = OnceLock::new();
    SENDER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel();
            thread::Builder::new()
                .name("wallspan-compose-preview".into())
                .spawn(move || preview_worker(rx))
                .expect("compose preview worker thread");
            tx
        })
        .clone()
}

fn preview_worker(rx: Receiver<PreviewJob>) {
    while let Ok(mut job) = rx.recv() {
        while let Ok(newer) = rx.try_recv() {
            job = newer;
        }

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
                    controller.as_mut().set_preview_status(QString::from(
                        "Preview ready · Apply is not implemented yet",
                    ));
                }
                Err(error) => {
                    controller.as_mut().set_preview_ready(false);
                    controller.as_mut().set_preview_status(QString::from(
                        format!("Preview failed: {error}").as_str(),
                    ));
                }
            }
        });
    }
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
