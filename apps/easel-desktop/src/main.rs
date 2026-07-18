// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Easel Qt Quick desktop application.

#![allow(
    clippy::float_cmp,
    clippy::needless_pass_by_value,
    clippy::unnecessary_box_returns
)]

mod app_controller;
mod compose_controller;
mod discover_controller;
mod display_session;
mod fixtures;
mod library_controller;
mod library_session;

use std::env;
use std::path::PathBuf;
use std::process;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    let args: Vec<String> = env::args().collect();
    let smoke = parse_smoke_outdir(&args);
    if let Some(outdir) = &smoke {
        let image_path = smoke_sample_image();
        if !image_path.is_file() {
            eprintln!("smoke sample image missing: {}", image_path.display());
            process::exit(2);
        }
        // macOS native Quick Controls crash during style customization / teardown
        // in headless CI; Fusion is stable for smoke captures.
        if env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
            // SAFETY: set before QGuiApplication is constructed.
            unsafe {
                env::set_var("QT_QUICK_CONTROLS_STYLE", "Fusion");
            }
        }
        display_session::use_fixture_arrangement();
        display_session::configure_smoke(outdir.clone(), image_path);
        eprintln!("smoke screenshot mode → {}", outdir.display());
    }

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/net/fritztech/easel/qml/main.qml"));
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
