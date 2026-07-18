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
    let smoke = match parse_smoke_args(&args) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            process::exit(2);
        }
    };
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

fn parse_smoke_args(args: &[String]) -> Result<Option<SmokeConfig>, String> {
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
            let value = iter
                .next()
                .ok_or_else(|| "invalid --smoke-views: missing value".to_string())?;
            if value.starts_with('-') {
                return Err(format!(
                    "invalid --smoke-views: missing value before '{value}'"
                ));
            }
            views_spec = Some(value.clone());
            continue;
        }
        if let Some(spec) = arg.strip_prefix("--smoke-views=") {
            if spec.is_empty() {
                return Err("invalid --smoke-views: missing value".to_string());
            }
            views_spec = Some(spec.to_string());
        }
    }
    let Some(out_dir) = out_dir else {
        return Ok(None);
    };
    let views = display_session::parse_smoke_views(views_spec.as_deref().unwrap_or(""))
        .map_err(|error| format!("invalid --smoke-views: {error}"))?;
    Ok(Some(SmokeConfig { out_dir, views }))
}

fn smoke_sample_image() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/smoke_source.png")
}

#[cfg(test)]
mod tests {
    use super::parse_smoke_args;

    fn args(parts: &[&str]) -> Vec<String> {
        std::iter::once("easel-desktop")
            .chain(parts.iter().copied())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn smoke_views_defaults_when_omitted() {
        let config = parse_smoke_args(&args(&["--smoke-screenshot", "/tmp/out"]))
            .unwrap()
            .expect("smoke config");
        assert_eq!(
            config.views,
            vec!["preview".to_string(), "compose".to_string()]
        );
    }

    #[test]
    fn smoke_views_missing_value_errors() {
        assert!(
            parse_smoke_args(&args(&["--smoke-screenshot", "/tmp/out", "--smoke-views"])).is_err()
        );
        assert!(
            parse_smoke_args(&args(&[
                "--smoke-screenshot",
                "/tmp/out",
                "--smoke-views",
                "--smoke-screenshot=/tmp/other"
            ]))
            .is_err()
        );
        assert!(
            parse_smoke_args(&args(&["--smoke-screenshot=/tmp/out", "--smoke-views="])).is_err()
        );
    }

    #[test]
    fn smoke_views_accepts_explicit_list() {
        let config = parse_smoke_args(&args(&[
            "--smoke-screenshot=/tmp/out",
            "--smoke-views=preview,discover",
        ]))
        .unwrap()
        .expect("smoke config");
        assert_eq!(
            config.views,
            vec!["preview".to_string(), "discover".to_string()]
        );
    }
}
