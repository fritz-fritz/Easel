// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! KDE Plasma 6 still-wallpaper backend via `org.kde.plasmashell`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use url::Url;

use crate::{
    BackendCapabilities, BackendError, DisplayWallpaper, WallpaperBackend, WallpaperOutput,
};

/// Plasma still-image / optional native-dynamic backend using session D-Bus scripting.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlasmaBackend;

impl WallpaperBackend for PlasmaBackend {
    fn id(&self) -> &'static str {
        "plasma6"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            per_display_images: true,
            virtual_desktop_image: false,
            activities: false,
            workspaces: false,
            lock_screen: false,
            cross_fade: false,
            native_dynamic_bundle: plasma_dynamic_plugin_id().is_some(),
        }
    }

    fn apply(&self, output: &WallpaperOutput) -> Result<(), BackendError> {
        match output {
            WallpaperOutput::PerDisplay(displays) => {
                for wallpaper in displays {
                    self.validate_output_path(&wallpaper.path)?;
                }
                let script = build_plasma_wallpaper_script(displays)?;
                evaluate_plasma_script(&script)
            }
            WallpaperOutput::NativeDynamic(displays) => {
                for wallpaper in displays {
                    self.validate_output_path(&wallpaper.path)?;
                }
                let plugin = plasma_dynamic_plugin_id().ok_or(BackendError::UnsupportedOutput)?;
                let script = build_plasma_native_dynamic_script(displays, plugin)?;
                evaluate_plasma_script(&script)
            }
            WallpaperOutput::VirtualDesktop(_) => Err(BackendError::UnsupportedOutput),
        }
    }
}

/// Returns the installed Plasma dynamic-wallpaper plugin id when present.
#[must_use]
pub fn plasma_dynamic_plugin_id() -> Option<&'static str> {
    static PLUGIN: OnceLock<Option<&'static str>> = OnceLock::new();
    *PLUGIN.get_or_init(detect_plasma_dynamic_plugin)
}

fn detect_plasma_dynamic_plugin() -> Option<&'static str> {
    // Prefer the widely used community dynamic wallpaper plugin; also accept
    // a few known alternate package ids if installed under plasma wallpapers.
    const CANDIDATES: &[&str] = &[
        "com.github.zzag.dynamic",
        "com.github.zzag.wallpaper.dynamic",
        "org.kde.plasma.dynamicwallpaper",
    ];
    let roots = plasma_wallpaper_roots();
    for id in CANDIDATES {
        for root in &roots {
            if root.join(id).is_dir() {
                return Some(*id);
            }
        }
    }
    None
}

fn plasma_wallpaper_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(home).join(".local/share/plasma/wallpapers"));
    }
    roots.push(PathBuf::from("/usr/share/plasma/wallpapers"));
    roots.push(PathBuf::from("/usr/local/share/plasma/wallpapers"));
    if let Ok(xdg) = std::env::var("XDG_DATA_DIRS") {
        for dir in xdg.split(':').filter(|part| !part.is_empty()) {
            roots.push(PathBuf::from(dir).join("plasma/wallpapers"));
        }
    }
    roots
}

/// Builds the JavaScript payload sent to `PlasmaShell.evaluateScript`.
///
/// Matching uses compositor geometry rather than Desktop index order.
pub fn build_plasma_wallpaper_script(
    displays: &[DisplayWallpaper],
) -> Result<String, BackendError> {
    build_plasma_plugin_script(displays, "org.kde.image")
}

/// Builds a Plasma script that hosts native dynamic HEIC packages per display.
pub fn build_plasma_native_dynamic_script(
    displays: &[DisplayWallpaper],
    plugin_id: &str,
) -> Result<String, BackendError> {
    build_plasma_plugin_script(displays, plugin_id)
}

fn build_plasma_plugin_script(
    displays: &[DisplayWallpaper],
    plugin_id: &str,
) -> Result<String, BackendError> {
    let mut assignments = String::new();
    for wallpaper in displays {
        let file_url = file_url_from_path(&wallpaper.path)?;
        let rect = wallpaper.logical_rect;
        assignments.push_str(&format!(
            r#"
setForGeometry({left}, {top}, {width}, {height}, "{url}");
"#,
            left = rect.x,
            top = rect.y,
            width = rect.width,
            height = rect.height,
            url = escape_js_string(&file_url),
        ));
    }

    Ok(format!(
        r#"
function screenGeometrySafe(screen) {{
    try {{
        return screenGeometry(screen);
    }} catch (e) {{
        return null;
    }}
}}

function setForGeometry(left, top, width, height, imageUrl) {{
    var all = desktops();
    for (var i = 0; i < all.length; ++i) {{
        var desktop = all[i];
        if (desktop.screen === -1) {{
            continue;
        }}
        var geometry = screenGeometrySafe(desktop.screen);
        if (!geometry) {{
            continue;
        }}
        if (geometry.x === left && geometry.y === top &&
            geometry.width === width && geometry.height === height) {{
            desktop.wallpaperPlugin = "{plugin}";
            desktop.currentConfigGroup = ["Wallpaper", "{plugin}", "General"];
            desktop.writeConfig("Image", imageUrl);
            desktop.reloadConfig();
            return;
        }}
    }}
    throw new Error("No Plasma desktop matched geometry " + left + "," + top + " " + width + "x" + height);
}}
{assignments}
"#,
        plugin = escape_js_string(plugin_id),
    ))
}

/// Escapes a string for embedding inside a double-quoted JavaScript literal.
#[must_use]
pub fn escape_js_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Builds the `qdbus6` argv used to run `PlasmaShell.evaluateScript`.
///
/// This is the **mutating apply** path: the script may write wallpaper config.
/// Session reachability probes must not use this; they go through
/// `plasma_available`, which only introspects the D-Bus object.
#[must_use]
pub fn plasma_evaluate_command(script: &str) -> Vec<String> {
    vec![
        "qdbus6".into(),
        "org.kde.plasmashell".into(),
        "/PlasmaShell".into(),
        "org.kde.PlasmaShell.evaluateScript".into(),
        script.into(),
    ]
}

fn file_url_from_path(path: &Path) -> Result<String, BackendError> {
    Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|()| {
            BackendError::Platform(format!(
                "path cannot be converted to a file URL: {}",
                path.display()
            ))
        })
}

fn evaluate_plasma_script(script: &str) -> Result<(), BackendError> {
    let qdbus6_args = plasma_evaluate_command(script);
    let status = Command::new(&qdbus6_args[0])
        .args(&qdbus6_args[1..])
        .status();
    match status {
        Ok(code) if code.success() => return Ok(()),
        Ok(code) => {
            return Err(BackendError::Platform(format!(
                "qdbus6 evaluateScript exited with {code}"
            )));
        }
        Err(_) => {}
    }

    let status = Command::new("qdbus")
        .args([
            "org.kde.plasmashell",
            "/PlasmaShell",
            "org.kde.PlasmaShell.evaluateScript",
            script,
        ])
        .status();
    match status {
        Ok(code) if code.success() => return Ok(()),
        Ok(code) => {
            return Err(BackendError::Platform(format!(
                "qdbus evaluateScript exited with {code}"
            )));
        }
        Err(_) => {}
    }

    let status = Command::new("dbus-send")
        .args([
            "--session",
            "--type=method_call",
            "--dest=org.kde.plasmashell",
            "/PlasmaShell",
            "org.kde.PlasmaShell.evaluateScript",
        ])
        .arg(format!("string:{script}"))
        .status();
    match status {
        Ok(code) if code.success() => Ok(()),
        Ok(code) => Err(BackendError::Platform(format!(
            "dbus-send evaluateScript exited with {code}"
        ))),
        Err(_) => Err(BackendError::Platform(
            "no qdbus6/qdbus/dbus-send available to talk to PlasmaShell".into(),
        )),
    }
}

#[cfg(not(windows))]
/// Returns whether PlasmaShell is reachable on the session bus.
pub(crate) fn plasma_available() -> bool {
    Command::new("qdbus6")
        .args(["org.kde.plasmashell", "/PlasmaShell"])
        .output()
        .map(|output| output.status.success())
        .or_else(|_| {
            Command::new("qdbus")
                .args(["org.kde.plasmashell", "/PlasmaShell"])
                .output()
                .map(|output| output.status.success())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::{DisplayId, LogicalRect};
    use std::path::PathBuf;

    fn sample_wallpaper(path: &str, rect: LogicalRect) -> DisplayWallpaper {
        DisplayWallpaper {
            display_id: DisplayId::from_u128(1),
            path: PathBuf::from(path),
            logical_rect: rect,
        }
    }

    #[test]
    fn script_contains_geometry_and_reload() {
        let path = std::env::temp_dir().join("easel-plasma-wall.png");
        let expected_url =
            Url::from_file_path(&path).expect("temp path should convert to file URL");
        let wallpaper = sample_wallpaper(
            path.to_str().expect("temp path is UTF-8"),
            LogicalRect {
                x: 2560,
                y: 0,
                width: 3840,
                height: 2160,
            },
        );
        let script = build_plasma_wallpaper_script(&[wallpaper]).expect("script");
        assert!(script.contains("setForGeometry(2560, 0, 3840, 2160"));
        assert!(script.contains("org.kde.image"));
        assert!(script.contains("reloadConfig"));
        assert!(script.contains(expected_url.as_str()));
    }

    #[test]
    fn native_dynamic_script_uses_plugin_id() {
        let path = std::env::temp_dir().join("easel-plasma-dyn.heic");
        let wallpaper = sample_wallpaper(
            path.to_str().expect("temp path is UTF-8"),
            LogicalRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
        );
        let script =
            build_plasma_native_dynamic_script(&[wallpaper], "com.github.zzag.dynamic").unwrap();
        assert!(script.contains("com.github.zzag.dynamic"));
        assert!(script.contains("writeConfig(\"Image\""));
    }

    #[test]
    fn escape_js_string_handles_quotes_and_slashes() {
        assert_eq!(escape_js_string(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn evaluate_command_targets_plasmashell() {
        let command = plasma_evaluate_command("print('hi')");
        assert_eq!(command[0], "qdbus6");
        assert_eq!(command[1], "org.kde.plasmashell");
        assert_eq!(command[3], "org.kde.PlasmaShell.evaluateScript");
        assert_eq!(command[4], "print('hi')");
    }
}
