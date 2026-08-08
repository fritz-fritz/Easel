// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Generic X11 still-wallpaper backend via `feh` (root pixmap).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::xrandr::{list_monitors, paths_in_monitor_order};
use crate::{
    BackendCapabilities, BackendError, DisplayWallpaper, WallpaperBackend, WallpaperOutput,
};

/// Generic X11 still backend that paints the root window through `feh`.
///
/// Preferred only when no desktop-native still backend (Plasma, XFCE, …) is
/// available. On DE sessions that own the backdrop (for example XFCE/`xfdesktop`),
/// prefer the DE adapter so settings persist in the desktop channel.
#[derive(Clone, Copy, Debug, Default)]
pub struct FehBackend;

impl WallpaperBackend for FehBackend {
    fn id(&self) -> &'static str {
        "x11-feh"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            per_display_images: true,
            virtual_desktop_image: true,
            activities: false,
            workspaces: false,
            lock_screen: false,
            cross_fade: false,
            native_dynamic_bundle: false,
        }
    }

    fn apply(&self, output: &WallpaperOutput) -> Result<(), BackendError> {
        match output {
            WallpaperOutput::PerDisplay(displays) => {
                for wallpaper in displays {
                    self.validate_output_path(&wallpaper.path)?;
                }
                apply_per_display(displays)
            }
            WallpaperOutput::VirtualDesktop(path) => {
                self.validate_output_path(path)?;
                apply_virtual_desktop(path)
            }
            WallpaperOutput::NativeDynamic(_) => Err(BackendError::UnsupportedOutput),
        }
    }
}

/// Returns whether `feh` is on `PATH` and a `DISPLAY` is configured.
#[must_use]
pub fn feh_available() -> bool {
    if std::env::var_os("DISPLAY").is_none() {
        return false;
    }
    Command::new("feh")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn apply_per_display(displays: &[DisplayWallpaper]) -> Result<(), BackendError> {
    let monitors = list_monitors()?;
    let paths = paths_in_monitor_order(&monitors, displays)?;
    run_feh_bg_fill(&paths)
}

fn apply_virtual_desktop(path: &Path) -> Result<(), BackendError> {
    let monitors = list_monitors()?;
    let paths: Vec<PathBuf> = monitors.iter().map(|_| path.to_path_buf()).collect();
    run_feh_bg_fill(&paths)
}

fn run_feh_bg_fill(paths: &[PathBuf]) -> Result<(), BackendError> {
    if paths.is_empty() {
        return Err(BackendError::Platform(
            "feh apply requires at least one wallpaper path".into(),
        ));
    }
    let mut command = Command::new("feh");
    // --no-fehbg: do not rewrite ~/.fehbg; Easel owns re-apply via its own store.
    command.args(["--no-fehbg", "--bg-fill"]);
    for path in paths {
        let absolute = absolute_utf8_path(path)?;
        command.arg(absolute);
    }
    let status = command
        .status()
        .map_err(|error| BackendError::Platform(format!("feh failed: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(BackendError::Platform(format!(
            "feh --bg-fill exited with {status}"
        )))
    }
}

fn absolute_utf8_path(path: &Path) -> Result<PathBuf, BackendError> {
    let absolute: PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| BackendError::Platform(format!("cwd unavailable: {error}")))?
            .join(path)
    };
    if absolute.to_str().is_none() {
        return Err(BackendError::Platform(
            "wallpaper path is not valid UTF-8".into(),
        ));
    }
    Ok(absolute)
}

/// Builds the feh argument list for tests without spawning `feh`.
#[cfg(test)]
pub fn plan_feh_bg_fill_args(
    monitors: &[crate::xrandr::XrandrMonitor],
    displays: &[DisplayWallpaper],
) -> Result<Vec<PathBuf>, BackendError> {
    paths_in_monitor_order(monitors, displays)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xrandr::parse_list_monitors;
    use easel_core::{DisplayId, LogicalRect};

    #[test]
    fn backend_id_and_capabilities() {
        let backend = FehBackend;
        assert_eq!(backend.id(), "x11-feh");
        assert!(backend.capabilities().per_display_images);
    }

    #[test]
    fn feh_args_follow_xrandr_order() {
        let monitors =
            parse_list_monitors("Monitors: 2\n 0: DP-1 640x360+0+0\n 1: DP-2 800x600+640+0\n");
        let displays = vec![
            DisplayWallpaper {
                display_id: DisplayId::from_u128(2),
                path: PathBuf::from("/tmp/right.png"),
                logical_rect: LogicalRect {
                    x: 640,
                    y: 0,
                    width: 800,
                    height: 600,
                },
            },
            DisplayWallpaper {
                display_id: DisplayId::from_u128(1),
                path: PathBuf::from("/tmp/left.png"),
                logical_rect: LogicalRect {
                    x: 0,
                    y: 0,
                    width: 640,
                    height: 360,
                },
            },
        ];
        let args = plan_feh_bg_fill_args(&monitors, &displays).expect("args");
        assert_eq!(
            args,
            vec![
                PathBuf::from("/tmp/left.png"),
                PathBuf::from("/tmp/right.png")
            ]
        );
    }
}
