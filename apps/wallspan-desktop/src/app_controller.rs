#![allow(clippy::too_many_arguments)]

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};

use crate::display_session::{self, ScreenProbe};

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
        #[qproperty(i32, display_count)]
        #[qproperty(bool, online_sources_available)]
        #[qproperty(QStringList, layout_model)]
        #[qproperty(QString, smoke_out_dir)]
        #[qproperty(QString, smoke_image_path)]
        type AppController = super::AppControllerRust;

        #[qinvokable]
        #[rust_name = "refresh_displays"]
        fn refreshDisplays(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "begin_screen_probe"]
        fn beginScreenProbe(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "add_screen_probe"]
        fn addScreenProbe(
            self: Pin<&mut Self>,
            name: QString,
            manufacturer: QString,
            model: QString,
            serial: QString,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            device_pixel_ratio: f64,
            physical_width_mm: f64,
            physical_height_mm: f64,
        );

        #[qinvokable]
        #[rust_name = "commit_screen_probe"]
        fn commitScreenProbe(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "use_fixture_displays"]
        fn useFixtureDisplays(self: Pin<&mut Self>);
    }
}

/// Presentation state only; screen probes arrive from QML Qt.application.screens.
pub struct AppControllerRust {
    status_text: QString,
    display_count: i32,
    online_sources_available: bool,
    layout_model: QStringList,
    smoke_out_dir: QString,
    smoke_image_path: QString,
    pending_probes: Vec<ScreenProbe>,
}

impl Default for AppControllerRust {
    fn default() -> Self {
        let layout = layout_qstring_list();
        let count = i32::try_from(display_session::current_displays().len()).unwrap_or(0);
        let smoke_out = display_session::smoke_paths()
            .map(|paths| paths.out_dir.to_string_lossy().into_owned())
            .unwrap_or_default();
        let smoke_image = display_session::smoke_paths()
            .map(|paths| paths.image_path.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !smoke_out.is_empty() {
            eprintln!("AppController smoke_out_dir={smoke_out}");
            eprintln!("AppController smoke_image_path={smoke_image}");
        }
        Self {
            status_text: "Ready".into(),
            display_count: count,
            online_sources_available: false,
            layout_model: layout,
            smoke_out_dir: smoke_out.into(),
            smoke_image_path: smoke_image.into(),
            pending_probes: Vec::new(),
        }
    }
}

impl qobject::AppController {
    fn refresh_displays(mut self: Pin<&mut Self>) {
        // QML `probeScreens()` collects Qt.application.screens then commits.
        self.as_mut()
            .set_status_text("Refresh displays from the toolbar after screens are probed".into());
        self.publish_layout();
    }

    fn begin_screen_probe(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().pending_probes.clear();
        self.as_mut().set_status_text("Probing Qt screens…".into());
    }

    fn add_screen_probe(
        mut self: Pin<&mut Self>,
        name: QString,
        manufacturer: QString,
        model: QString,
        serial: QString,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        device_pixel_ratio: f64,
        physical_width_mm: f64,
        physical_height_mm: f64,
    ) {
        let width = u32::try_from(width.max(0)).unwrap_or(0);
        let height = u32::try_from(height.max(0)).unwrap_or(0);
        self.as_mut().rust_mut().pending_probes.push(ScreenProbe {
            name: name.to_string(),
            manufacturer: manufacturer.to_string(),
            model: model.to_string(),
            serial: serial.to_string(),
            x,
            y,
            width,
            height,
            device_pixel_ratio,
            physical_width_mm,
            physical_height_mm,
        });
    }

    fn commit_screen_probe(mut self: Pin<&mut Self>) {
        let probes = std::mem::take(&mut self.as_mut().rust_mut().pending_probes);
        match display_session::replace_from_probes(probes) {
            Ok(arrangement) => {
                let count = i32::try_from(arrangement.displays.len()).unwrap_or(0);
                self.as_mut().set_display_count(count);
                self.as_mut().set_status_text(
                    format!("Detected {count} display(s); arrangement saved").into(),
                );
                self.publish_layout();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(format!("Display probe failed: {error}").into());
            }
        }
    }

    fn use_fixture_displays(mut self: Pin<&mut Self>) {
        display_session::use_fixture_arrangement();
        let count = i32::try_from(display_session::current_displays().len()).unwrap_or(0);
        self.as_mut().set_display_count(count);
        self.as_mut()
            .set_status_text("Using fixture three-monitor layout".into());
        self.publish_layout();
    }

    fn publish_layout(mut self: Pin<&mut Self>) {
        self.as_mut().set_layout_model(layout_qstring_list());
    }
}

fn layout_qstring_list() -> QStringList {
    let mut list = QStringList::default();
    for row in display_session::layout_preview_model() {
        list.append_clone(&QString::from(row.as_str()));
    }
    list
}
