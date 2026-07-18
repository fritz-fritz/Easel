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
mod apply_service;
mod automation_controller;
mod automation_session;
mod compose_controller;
mod discover_controller;
mod display_session;
mod fixtures;
mod library_controller;
mod library_session;
mod profile_controller;

use std::env;
use std::path::PathBuf;
use std::process;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    let args: Vec<String> = env::args().collect();
    let smoke = parse_smoke_args(&args);
    if let Some(config) = &smoke {
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
        display_session::configure_smoke(config.out_dir.clone(), image_path, config.views.clone());
        eprintln!(
            "smoke screenshot mode → {} (views: {})",
            config.out_dir.display(),
            config.views.join(",")
        );
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

#[derive(Clone, Debug)]
struct SmokeConfig {
    out_dir: PathBuf,
    views: Vec<String>,
}

fn parse_smoke_args(args: &[String]) -> Option<SmokeConfig> {
    let mut out_dir: Option<PathBuf> = None;
    let mut views_spec: Option<String> = None;
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--smoke-screenshot" {
            out_dir = iter.next().map(PathBuf::from);
            continue;
        }
        if let Some(path) = arg.strip_prefix("--smoke-screenshot=") {
            out_dir = Some(PathBuf::from(path));
            continue;
        }
        if arg == "--smoke-views" {
            views_spec = iter.next().cloned();
            continue;
        }
        if let Some(spec) = arg.strip_prefix("--smoke-views=") {
            views_spec = Some(spec.to_string());
        }
    }
    let out_dir = out_dir?;
    let views = match display_session::parse_smoke_views(views_spec.as_deref().unwrap_or("")) {
        Ok(views) => views,
        Err(error) => {
            eprintln!("invalid --smoke-views: {error}");
            process::exit(2);
        }
    };
    Some(SmokeConfig { out_dir, views })
}

fn smoke_sample_image() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/smoke_source.png")
}
