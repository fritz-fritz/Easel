// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! GNOME-family still-wallpaper backend via `gsettings`.

use std::path::{Path, PathBuf};
use std::process::Command;

use image::imageops::{FilterType, overlay, resize};
use image::{Rgba, RgbaImage};
use url::Url;

use crate::{
    BackendCapabilities, BackendError, DisplayWallpaper, WallpaperBackend, WallpaperOutput,
};

/// GNOME still backend. Multi-monitor spanning is applied as one `spanned`
/// virtual-desktop image (ADR 0012); the public schema has no per-monitor URI.
#[derive(Clone, Copy, Debug, Default)]
pub struct GnomeBackend;

impl WallpaperBackend for GnomeBackend {
    fn id(&self) -> &'static str {
        "gnome-gsettings"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            // Appearance is delivered via a spanned composite of per-display crops.
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
                if displays.len() == 1 {
                    set_background_image(&displays[0].path, "zoom")
                } else {
                    let spanned = composite_spanned(displays)?;
                    set_background_image(&spanned, "spanned")
                }
            }
            WallpaperOutput::VirtualDesktop(path) => {
                self.validate_output_path(path)?;
                set_background_image(path, "spanned")
            }
            WallpaperOutput::NativeDynamic(_) => Err(BackendError::UnsupportedOutput),
        }
    }
}

/// Returns whether this looks like a GNOME-family session with a writable background schema.
///
/// `gsettings` alone is not enough — XFCE images also ship it for appearance probes.
#[must_use]
pub fn gnome_available() -> bool {
    if !gnome_session_hint() {
        return false;
    }
    Command::new("gsettings")
        .args(["get", "org.gnome.desktop.background", "picture-uri"])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn gnome_session_hint() -> bool {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let session = std::env::var("DESKTOP_SESSION").unwrap_or_default();
    if desktop_env_looks_like_gnome(&desktop, &session) {
        return true;
    }
    // Last resort: session bus has gnome-shell (covers nested/unusual desktop vars).
    Command::new("gdbus")
        .args([
            "introspect",
            "--session",
            "--dest",
            "org.gnome.Shell",
            "--object-path",
            "/org/gnome/Shell",
        ])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Env-only GNOME-family check (no D-Bus). Does not treat a bare `ubuntu` token as
/// GNOME — Ubuntu flavors set `ubuntu:GNOME` / `XFCE` / etc.; matching `ubuntu` alone
/// would false-positive non-GNOME sessions.
fn desktop_env_looks_like_gnome(xdg_current_desktop: &str, desktop_session: &str) -> bool {
    if xdg_current_desktop
        .split(':')
        .any(|part| part.eq_ignore_ascii_case("GNOME"))
    {
        return true;
    }
    desktop_session.to_ascii_lowercase().contains("gnome")
}

fn set_background_image(path: &Path, options: &str) -> Result<(), BackendError> {
    let absolute = absolute_path(path)?;
    let uri = Url::from_file_path(&absolute).map_err(|()| {
        BackendError::Platform(format!(
            "could not convert wallpaper path to file URI: {}",
            absolute.display()
        ))
    })?;
    // gsettings values are serialized GVariants; strings need quotes.
    let uri_variant = gvariant_string(uri.as_str());
    gsettings_set(&["org.gnome.desktop.background", "picture-options", options])?;
    gsettings_set(&[
        "org.gnome.desktop.background",
        "picture-uri",
        uri_variant.as_str(),
    ])?;
    // Dark-style sessions read picture-uri-dark when present.
    let _ = gsettings_set(&[
        "org.gnome.desktop.background",
        "picture-uri-dark",
        uri_variant.as_str(),
    ]);
    Ok(())
}

fn gvariant_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn gsettings_set(args: &[&str]) -> Result<(), BackendError> {
    let status = Command::new("gsettings")
        .arg("set")
        .args(args)
        .status()
        .map_err(|error| BackendError::Platform(format!("gsettings failed: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(BackendError::Platform(format!(
            "gsettings set {} exited with {status}",
            args.join(" ")
        )))
    }
}

/// Composites per-display crops onto one virtual-desktop image for `picture-options=spanned`.
fn composite_spanned(displays: &[DisplayWallpaper]) -> Result<PathBuf, BackendError> {
    if displays.is_empty() {
        return Err(BackendError::UnsupportedOutput);
    }
    let min_x = displays
        .iter()
        .map(|display| display.logical_rect.x)
        .min()
        .unwrap_or(0);
    let min_y = displays
        .iter()
        .map(|display| display.logical_rect.y)
        .min()
        .unwrap_or(0);
    let max_x = displays
        .iter()
        .map(|display| {
            display.logical_rect.x + i32::try_from(display.logical_rect.width).unwrap_or(0)
        })
        .max()
        .unwrap_or(min_x + 1);
    let max_y = displays
        .iter()
        .map(|display| {
            display.logical_rect.y + i32::try_from(display.logical_rect.height).unwrap_or(0)
        })
        .max()
        .unwrap_or(min_y + 1);
    let width = u32::try_from((max_x - min_x).max(1)).unwrap_or(1);
    let height = u32::try_from((max_y - min_y).max(1)).unwrap_or(1);

    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));
    for wallpaper in displays {
        let source = image::open(&wallpaper.path)
            .map_err(|error| BackendError::Platform(format!("decode wallpaper: {error}")))?
            .to_rgba8();
        let target_w = wallpaper.logical_rect.width.max(1);
        let target_h = wallpaper.logical_rect.height.max(1);
        let tile = if source.width() == target_w && source.height() == target_h {
            source
        } else {
            resize(&source, target_w, target_h, FilterType::Lanczos3)
        };
        let dest_x = i64::from(wallpaper.logical_rect.x - min_x);
        let dest_y = i64::from(wallpaper.logical_rect.y - min_y);
        overlay(&mut canvas, &tile, dest_x, dest_y);
    }

    let out = std::env::temp_dir().join(format!("easel-gnome-spanned-{}.png", std::process::id()));
    canvas
        .save(&out)
        .map_err(|error| BackendError::Platform(format!("write spanned wallpaper: {error}")))?;
    Ok(out)
}

fn absolute_path(path: &Path) -> Result<PathBuf, BackendError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map_err(|error| BackendError::Platform(format!("cwd unavailable: {error}")))
            .map(|cwd| cwd.join(path))
    }
}

/// Builds a spanned canvas size for tests without decoding images.
#[cfg(test)]
pub fn spanned_canvas_size(displays: &[DisplayWallpaper]) -> (u32, u32) {
    let min_x = displays
        .iter()
        .map(|display| display.logical_rect.x)
        .min()
        .unwrap_or(0);
    let min_y = displays
        .iter()
        .map(|display| display.logical_rect.y)
        .min()
        .unwrap_or(0);
    let max_x = displays
        .iter()
        .map(|display| {
            display.logical_rect.x + i32::try_from(display.logical_rect.width).unwrap_or(0)
        })
        .max()
        .unwrap_or(min_x + 1);
    let max_y = displays
        .iter()
        .map(|display| {
            display.logical_rect.y + i32::try_from(display.logical_rect.height).unwrap_or(0)
        })
        .max()
        .unwrap_or(min_y + 1);
    (
        u32::try_from((max_x - min_x).max(1)).unwrap_or(1),
        u32::try_from((max_y - min_y).max(1)).unwrap_or(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::{DisplayId, LogicalRect};

    #[test]
    fn backend_id_and_capabilities() {
        let backend = GnomeBackend;
        assert_eq!(backend.id(), "gnome-gsettings");
        assert!(backend.capabilities().per_display_images);
        assert!(backend.capabilities().virtual_desktop_image);
        assert!(!backend.capabilities().native_dynamic_bundle);
    }

    #[test]
    fn spanned_canvas_covers_staggered_monitors() {
        let displays = vec![
            DisplayWallpaper {
                display_id: DisplayId::from_u128(1),
                path: PathBuf::from("/tmp/a.png"),
                logical_rect: LogicalRect {
                    x: 0,
                    y: 200,
                    width: 3840,
                    height: 2160,
                },
            },
            DisplayWallpaper {
                display_id: DisplayId::from_u128(2),
                path: PathBuf::from("/tmp/b.png"),
                logical_rect: LogicalRect {
                    x: 3840,
                    y: 0,
                    width: 3440,
                    height: 1440,
                },
            },
            DisplayWallpaper {
                display_id: DisplayId::from_u128(3),
                path: PathBuf::from("/tmp/c.png"),
                logical_rect: LogicalRect {
                    x: 7280,
                    y: 400,
                    width: 1920,
                    height: 1080,
                },
            },
        ];
        assert_eq!(spanned_canvas_size(&displays), (9200, 2360));
    }

    #[test]
    fn desktop_env_requires_gnome_token_not_bare_ubuntu() {
        assert!(desktop_env_looks_like_gnome("ubuntu:GNOME", "ubuntu"));
        assert!(desktop_env_looks_like_gnome("GNOME", "gnome"));
        assert!(desktop_env_looks_like_gnome(
            "GNOME-Flashback:GNOME",
            "gnome-flashback"
        ));
        assert!(!desktop_env_looks_like_gnome("ubuntu", "ubuntu"));
        assert!(!desktop_env_looks_like_gnome("XFCE", "xfce"));
        assert!(!desktop_env_looks_like_gnome("KDE", "plasma"));
    }

    #[test]
    fn gvariant_string_quotes_file_uri() {
        assert_eq!(
            gvariant_string("file:///tmp/wall.png"),
            "'file:///tmp/wall.png'"
        );
    }

    #[test]
    fn composite_spanned_writes_bounding_box_png() {
        let dir =
            std::env::temp_dir().join(format!("easel-gnome-composite-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let left = dir.join("left.png");
        let right = dir.join("right.png");
        RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]))
            .save(&left)
            .expect("left");
        RgbaImage::from_pixel(6, 4, Rgba([0, 255, 0, 255]))
            .save(&right)
            .expect("right");
        let displays = vec![
            DisplayWallpaper {
                display_id: DisplayId::from_u128(1),
                path: left,
                logical_rect: LogicalRect {
                    x: 0,
                    y: 2,
                    width: 4,
                    height: 4,
                },
            },
            DisplayWallpaper {
                display_id: DisplayId::from_u128(2),
                path: right,
                logical_rect: LogicalRect {
                    x: 4,
                    y: 0,
                    width: 6,
                    height: 4,
                },
            },
        ];
        let out = composite_spanned(&displays).expect("composite");
        let img = image::open(&out).expect("open").to_rgba8();
        assert_eq!(img.dimensions(), (10, 6));
        assert_eq!(*img.get_pixel(0, 2), Rgba([255, 0, 0, 255]));
        assert_eq!(*img.get_pixel(4, 0), Rgba([0, 255, 0, 255]));
        assert_eq!(*img.get_pixel(0, 0), Rgba([0, 0, 0, 255]));
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
