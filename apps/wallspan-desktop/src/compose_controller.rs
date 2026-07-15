//! Compose-page presentation model and background preview rendering.

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
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
        }
    }
}

impl qobject::ComposeController {
    fn set_source_path_from_url(mut self: Pin<&mut Self>, url: QString) {
        let path = strip_file_url(&url.to_string());
        self.as_mut().set_source_path(QString::from(path.as_str()));
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

        self.as_mut()
            .set_preview_status(QString::from("Rendering preview…"));

        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let result = RasterJob {
                request,
                output_dir,
            }
            .execute();

            let _ = qt_thread.queue(move |mut controller| {
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
                            let url = format!("file://{}", output.path.display());
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

fn strip_file_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(path) = trimmed.strip_prefix("file://") {
        let path = path.strip_prefix("localhost").unwrap_or(path);
        return path.replace("%20", " ");
    }
    trimmed.to_string()
}

fn preview_cache_dir() -> PathBuf {
    std::env::temp_dir()
        .join("wallspan")
        .join("compose-preview")
}
