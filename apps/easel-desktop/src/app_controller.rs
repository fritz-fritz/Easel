// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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
        #[qproperty(bool, physical_preview)]
        #[qproperty(QString, selected_display_id)]
        #[qproperty(f64, selected_origin_x_mm)]
        #[qproperty(f64, selected_origin_y_mm)]
        #[qproperty(f64, selected_width_mm)]
        #[qproperty(f64, selected_height_mm)]
        #[qproperty(f64, selected_bezel_mm)]
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

        /// Exits the process immediately after a smoke screenshot (skips Qt teardown).
        #[qinvokable]
        #[rust_name = "force_smoke_exit"]
        fn forceSmokeExit(self: Pin<&mut Self>, code: i32);

        #[qinvokable]
        #[rust_name = "set_physical_preview_enabled"]
        fn setPhysicalPreviewEnabled(self: Pin<&mut Self>, enabled: bool);

        #[qinvokable]
        #[rust_name = "select_display"]
        fn selectDisplay(self: Pin<&mut Self>, id: QString);

        #[qinvokable]
        #[rust_name = "move_selected_display"]
        fn moveSelectedDisplay(self: Pin<&mut Self>, origin_x_mm: f64, origin_y_mm: f64);

        #[qinvokable]
        #[rust_name = "apply_selected_size"]
        fn applySelectedSize(self: Pin<&mut Self>, width_mm: f64, height_mm: f64);

        #[qinvokable]
        #[rust_name = "apply_selected_bezel"]
        fn applySelectedBezel(self: Pin<&mut Self>, bezel_mm: f64);
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
    physical_preview: bool,
    selected_display_id: QString,
    selected_origin_x_mm: f64,
    selected_origin_y_mm: f64,
    selected_width_mm: f64,
    selected_height_mm: f64,
    selected_bezel_mm: f64,
    pending_probes: Vec<ScreenProbe>,
}

impl Default for AppControllerRust {
    fn default() -> Self {
        let layout = layout_qstring_list(true);
        let count = i32::try_from(display_session::current_displays().len()).unwrap_or(0);
        let smoke_out = display_session::smoke_paths()
            .map(|paths| paths.out_dir.to_string_lossy().into_owned())
            .unwrap_or_default();
        let smoke_image = display_session::smoke_paths()
            .map(|paths| paths.image_path.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            status_text: "Ready".into(),
            display_count: count,
            online_sources_available: false,
            layout_model: layout,
            smoke_out_dir: smoke_out.into(),
            smoke_image_path: smoke_image.into(),
            physical_preview: true,
            selected_display_id: QString::default(),
            selected_origin_x_mm: 0.0,
            selected_origin_y_mm: 0.0,
            selected_width_mm: 0.0,
            selected_height_mm: 0.0,
            selected_bezel_mm: 0.0,
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

    fn force_smoke_exit(self: Pin<&mut Self>, code: i32) {
        let _ = self;
        // Immediate process exit avoids intermittent Qt teardown crashes on macOS
        // after grabToImage (native Quick Controls style + ApplicationWindow shutdown).
        std::process::exit(code);
    }

    fn set_physical_preview_enabled(mut self: Pin<&mut Self>, enabled: bool) {
        self.as_mut().set_physical_preview(enabled);
        self.publish_layout();
    }

    fn select_display(mut self: Pin<&mut Self>, id: QString) {
        let id_string = id.to_string();
        self.as_mut().set_selected_display_id(id);
        if let Some(display) = display_session::current_displays()
            .into_iter()
            .find(|display| display.id.to_hyphenated_string() == id_string)
        {
            self.as_mut()
                .set_selected_origin_x_mm(display.physical_origin.x.0);
            self.as_mut()
                .set_selected_origin_y_mm(display.physical_origin.y.0);
            self.as_mut()
                .set_selected_width_mm(display.physical_size.width.0);
            self.as_mut()
                .set_selected_height_mm(display.physical_size.height.0);
            self.as_mut().set_selected_bezel_mm(display.bezel.left.0);
            self.as_mut().set_status_text(
                format!(
                    "Selected {}",
                    display
                        .connector_name
                        .unwrap_or_else(|| display.id.to_hyphenated_string())
                )
                .into(),
            );
        }
    }

    fn move_selected_display(mut self: Pin<&mut Self>, origin_x_mm: f64, origin_y_mm: f64) {
        let id = self.selected_display_id().to_string();
        if id.trim().is_empty() {
            return;
        }
        match display_session::move_display_physical(&id, origin_x_mm, origin_y_mm, 12.0) {
            Ok(()) => {
                self.as_mut()
                    .set_status_text("Updated display position".into());
                self.publish_layout();
                // Re-read snapped coordinates into the numeric editors.
                let selected = QString::from(id.as_str());
                self.select_display(selected);
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(format!("Move failed: {error}").into());
            }
        }
    }

    fn apply_selected_size(mut self: Pin<&mut Self>, width_mm: f64, height_mm: f64) {
        let id = self.selected_display_id().to_string();
        if id.trim().is_empty() {
            return;
        }
        match display_session::override_display_size(&id, width_mm, height_mm) {
            Ok(()) => {
                self.as_mut().set_selected_width_mm(width_mm);
                self.as_mut().set_selected_height_mm(height_mm);
                self.as_mut()
                    .set_status_text("Updated physical size override".into());
                self.publish_layout();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(format!("Size update failed: {error}").into());
            }
        }
    }

    fn apply_selected_bezel(mut self: Pin<&mut Self>, bezel_mm: f64) {
        let id = self.selected_display_id().to_string();
        if id.trim().is_empty() {
            return;
        }
        match display_session::set_display_bezel(&id, bezel_mm) {
            Ok(()) => {
                self.as_mut().set_selected_bezel_mm(bezel_mm);
                self.as_mut().set_status_text("Updated bezel insets".into());
                self.publish_layout();
            }
            Err(error) => {
                self.as_mut()
                    .set_status_text(format!("Bezel update failed: {error}").into());
            }
        }
    }

    fn publish_layout(mut self: Pin<&mut Self>) {
        let physical = *self.as_ref().physical_preview();
        self.as_mut()
            .set_layout_model(layout_qstring_list(physical));
    }
}

fn layout_qstring_list(physical: bool) -> QStringList {
    let mut list = QStringList::default();
    for row in display_session::layout_preview_model_mode(physical) {
        list.append_clone(&QString::from(row.as_str()));
    }
    list
}
