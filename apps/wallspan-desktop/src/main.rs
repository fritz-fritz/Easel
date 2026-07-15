//! Wallspan Qt Quick desktop application.

#![allow(
    clippy::float_cmp,
    clippy::needless_pass_by_value,
    clippy::unnecessary_box_returns
)]

mod app_controller;
mod compose_controller;
mod display_session;
mod fixtures;

use std::env;
use std::path::PathBuf;
use std::process;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Some(outdir) = parse_smoke_outdir(&args) {
        let image_path = smoke_sample_image();
        if !image_path.is_file() {
            eprintln!("smoke sample image missing: {}", image_path.display());
            process::exit(2);
        }
        display_session::use_fixture_arrangement();
        display_session::configure_smoke(outdir.clone(), image_path);
        eprintln!("smoke screenshot mode → {}", outdir.display());
    }

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(
            "qrc:/qt/qml/net/fritztech/wallspan/qml/main.qml",
        ));
    }

    if let Some(app) = app.as_mut() {
        let code = app.exec();
        process::exit(code);
    }
}

fn parse_smoke_outdir(args: &[String]) -> Option<PathBuf> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--smoke-screenshot" {
            return iter.next().map(PathBuf::from);
        }
        if let Some(path) = arg.strip_prefix("--smoke-screenshot=") {
            return Some(PathBuf::from(path));
        }
    }
    None
}

fn smoke_sample_image() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/smoke_source.png")
}
