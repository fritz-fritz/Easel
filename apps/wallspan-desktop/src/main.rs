//! Wallspan Qt Quick desktop application.

#![allow(
    clippy::float_cmp,
    clippy::needless_pass_by_value,
    clippy::unnecessary_box_returns
)]

mod app_controller;
mod compose_controller;
mod fixtures;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(
            "qrc:/qt/qml/net/fritztech/wallspan/qml/main.qml",
        ));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
