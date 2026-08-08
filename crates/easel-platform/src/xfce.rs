// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! XFCE still-wallpaper backend via `xfconf-query` / `xfce4-desktop`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::xrandr::{list_monitors, plan_monitor_assignments};
use crate::{
    BackendCapabilities, BackendError, DisplayWallpaper, WallpaperBackend, WallpaperOutput,
};

/// XFCE image-style: Zoomed (matches typical desktop defaults; Easel crops are display-sized).
const IMAGE_STYLE_ZOOMED: i32 = 5;

/// XFCE still-image backend using the `xfce4-desktop` xfconf channel.
#[derive(Clone, Copy, Debug, Default)]
pub struct XfceBackend;

impl WallpaperBackend for XfceBackend {
    fn id(&self) -> &'static str {
        "xfce-xfconf"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            per_display_images: true,
            virtual_desktop_image: true,
            activities: false,
            // Per-workspace wallpaper properties exist, but Easel does not yet track
            // workspace switches as first-class profile state (Stage 7 workspace slice).
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

/// Returns whether the XFCE desktop channel is reachable via `xfconf-query`.
#[must_use]
pub fn xfce_available() -> bool {
    Command::new("xfconf-query")
        .args(["-c", "xfce4-desktop", "-l"])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn apply_per_display(displays: &[DisplayWallpaper]) -> Result<(), BackendError> {
    let monitors = list_monitors()?;
    let planned = plan_monitor_assignments(&monitors, displays)?;
    for (monitor, path) in planned {
        set_monitor_image(&monitor, &path)?;
    }
    Ok(())
}

fn apply_virtual_desktop(path: &Path) -> Result<(), BackendError> {
    let monitors = list_monitors()?;
    for monitor in monitors {
        set_monitor_image(&monitor.name, path)?;
    }
    Ok(())
}

fn set_monitor_image(monitor: &str, path: &Path) -> Result<(), BackendError> {
    let absolute = absolute_utf8_path(path)?;
    let workspaces = existing_workspaces(monitor);
    let targets = if workspaces.is_empty() {
        vec![0_u32]
    } else {
        workspaces
    };
    for workspace in targets {
        let image_prop =
            format!("/backdrop/screen0/monitor{monitor}/workspace{workspace}/last-image");
        let style_prop =
            format!("/backdrop/screen0/monitor{monitor}/workspace{workspace}/image-style");
        xfconf_set_string(&image_prop, &absolute)?;
        xfconf_set_int(&style_prop, IMAGE_STYLE_ZOOMED)?;
    }
    Ok(())
}

fn existing_workspaces(monitor: &str) -> Vec<u32> {
    let output = Command::new("xfconf-query")
        .args(["-c", "xfce4-desktop", "-l"])
        .output()
        .ok();
    let Some(output) = output.filter(|output| output.status.success()) else {
        return Vec::new();
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let needle = format!("/backdrop/screen0/monitor{monitor}/workspace");
    let mut workspaces = Vec::new();
    for line in stdout.lines() {
        let Some(rest) = line.trim().strip_prefix(&needle) else {
            continue;
        };
        let Some((index, _)) = rest.split_once('/') else {
            continue;
        };
        if let Ok(workspace) = index.parse::<u32>()
            && !workspaces.contains(&workspace)
        {
            workspaces.push(workspace);
        }
    }
    workspaces.sort_unstable();
    workspaces
}

fn xfconf_set_string(property: &str, value: &str) -> Result<(), BackendError> {
    // Create-or-set: -n creates when missing; harmless when the property exists.
    let status = Command::new("xfconf-query")
        .args([
            "-c",
            "xfce4-desktop",
            "-p",
            property,
            "-n",
            "-t",
            "string",
            "-s",
            value,
        ])
        .status()
        .map_err(|error| BackendError::Platform(format!("xfconf-query failed: {error}")))?;
    if status.success() {
        return Ok(());
    }
    // Property may already exist with a different type invocation path; retry without -n.
    let status = Command::new("xfconf-query")
        .args(["-c", "xfce4-desktop", "-p", property, "-s", value])
        .status()
        .map_err(|error| BackendError::Platform(format!("xfconf-query failed: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(BackendError::Platform(format!(
            "xfconf-query could not set {property} (exit {status})"
        )))
    }
}

fn xfconf_set_int(property: &str, value: i32) -> Result<(), BackendError> {
    let value_str = value.to_string();
    let status = Command::new("xfconf-query")
        .args([
            "-c",
            "xfce4-desktop",
            "-p",
            property,
            "-n",
            "-t",
            "int",
            "-s",
            &value_str,
        ])
        .status()
        .map_err(|error| BackendError::Platform(format!("xfconf-query failed: {error}")))?;
    if status.success() {
        return Ok(());
    }
    let status = Command::new("xfconf-query")
        .args(["-c", "xfce4-desktop", "-p", property, "-s", &value_str])
        .status()
        .map_err(|error| BackendError::Platform(format!("xfconf-query failed: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(BackendError::Platform(format!(
            "xfconf-query could not set {property} (exit {status})"
        )))
    }
}

fn absolute_utf8_path(path: &Path) -> Result<String, BackendError> {
    let absolute: PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| BackendError::Platform(format!("cwd unavailable: {error}")))?
            .join(path)
    };
    absolute
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| BackendError::Platform("wallpaper path is not valid UTF-8".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_id_and_capabilities() {
        let backend = XfceBackend;
        assert_eq!(backend.id(), "xfce-xfconf");
        let caps = backend.capabilities();
        assert!(caps.per_display_images);
        assert!(caps.virtual_desktop_image);
        assert!(!caps.native_dynamic_bundle);
    }

    #[test]
    fn parses_workspace_indices_from_property_list() {
        let listing = "\
/backdrop/screen0/monitorDP-1/workspace0/last-image
/backdrop/screen0/monitorDP-1/workspace0/image-style
/backdrop/screen0/monitorDP-1/workspace2/last-image
/backdrop/screen0/monitorDP-2/workspace0/last-image
";
        let mut workspaces = Vec::new();
        let needle = "/backdrop/screen0/monitorDP-1/workspace";
        for line in listing.lines() {
            let Some(rest) = line.trim().strip_prefix(needle) else {
                continue;
            };
            let Some((index, _)) = rest.split_once('/') else {
                continue;
            };
            if let Ok(workspace) = index.parse::<u32>()
                && !workspaces.contains(&workspace)
            {
                workspaces.push(workspace);
            }
        }
        workspaces.sort_unstable();
        assert_eq!(workspaces, vec![0, 2]);
    }

    /// Opt-in live XFCE apply check. Skipped unless `EASEL_XFCE_LIVE_APPLY=1` so
    /// default `cargo test` never mutates the desktop session.
    #[test]
    fn live_xfce_apply_matches_three_display_fixture() {
        use easel_core::DisplayId;
        use std::fs;

        if std::env::var("EASEL_XFCE_LIVE_APPLY").ok().as_deref() != Some("1") {
            return;
        }
        if std::env::var_os("DISPLAY").is_none() || !xfce_available() {
            return;
        }
        let Ok(monitors) = crate::xrandr::list_monitors() else {
            return;
        };
        if monitors.is_empty() {
            return;
        }

        // Minimal valid 1×1 PNG bytes.
        let png = [
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xfe,
            0xd4, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        let dir = std::env::temp_dir().join(format!("easel-xfce-apply-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("tmpdir");
        let mut wallpapers = Vec::new();
        for (index, monitor) in monitors.iter().enumerate() {
            let path = dir.join(format!("{}.png", monitor.name));
            fs::write(&path, png).expect("write png");
            wallpapers.push(DisplayWallpaper {
                display_id: DisplayId::from_u128(u128::try_from(index + 1).unwrap_or(1)),
                path,
                logical_rect: monitor.rect,
            });
        }
        XfceBackend
            .apply(&WallpaperOutput::PerDisplay(wallpapers))
            .expect("xfce apply");
        for monitor in &monitors {
            let prop = format!(
                "/backdrop/screen0/monitor{}/workspace0/last-image",
                monitor.name
            );
            let output = Command::new("xfconf-query")
                .args(["-c", "xfce4-desktop", "-p", &prop])
                .output()
                .expect("xfconf get");
            assert!(
                output.status.success(),
                "missing backdrop property for {}",
                monitor.name
            );
            let value = String::from_utf8_lossy(&output.stdout);
            assert!(
                value.contains(&format!("{}.png", monitor.name)),
                "unexpected last-image for {}: {value}",
                monitor.name
            );
        }
        let _ = fs::remove_dir_all(dir);
    }
}
