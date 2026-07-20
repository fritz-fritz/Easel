// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared still-frame state file for the Easel Plasma wallpaper plugin (ADR 0008).
//!
//! Desktop automation writes this file after rendering per-display stills. The
//! Plasma plugin watches it and updates its `Image` source without requiring a
//! `PlasmaShell.evaluateScript` call on every dense-solar tick.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use easel_core::LogicalRect;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{BackendError, DisplayWallpaper};

/// Schema version for [`PlasmaWallpaperState`].
pub const PLASMA_WALLPAPER_STATE_VERSION: u32 = 1;

/// Relative directory under the Easel data dir that holds the state file.
pub const PLASMA_WALLPAPER_STATE_DIR: &str = "plasma-wallpaper";

/// File name written by desktop automation and watched by the plugin.
pub const PLASMA_WALLPAPER_STATE_FILE: &str = "active.json";

/// One display's active still frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlasmaWallpaperDisplayState {
    /// Logical compositor rectangle used to match a Plasma containment.
    pub geometry: PlasmaWallpaperGeometry,
    /// Absolute `file://` URL or filesystem path to the still image.
    pub image: String,
}

/// Integer geometry matching [`LogicalRect`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlasmaWallpaperGeometry {
    /// Left edge in logical pixels.
    pub x: i32,
    /// Top edge in logical pixels.
    pub y: i32,
    /// Width in logical pixels.
    pub width: u32,
    /// Height in logical pixels.
    pub height: u32,
}

impl From<LogicalRect> for PlasmaWallpaperGeometry {
    fn from(rect: LogicalRect) -> Self {
        Self {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

impl PlasmaWallpaperGeometry {
    /// Returns whether this geometry matches a Plasma screen rectangle.
    #[must_use]
    pub const fn matches(self, x: i32, y: i32, width: u32, height: u32) -> bool {
        self.x == x && self.y == y && self.width == width && self.height == height
    }
}

/// Root document written to [`PLASMA_WALLPAPER_STATE_FILE`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlasmaWallpaperState {
    /// Schema version.
    pub version: u32,
    /// Unix timestamp when the document was written.
    pub updated_at: u64,
    /// Per-display still frames.
    pub displays: Vec<PlasmaWallpaperDisplayState>,
}

impl PlasmaWallpaperState {
    /// Builds state from renderer output destined for Plasma.
    #[must_use]
    pub fn from_wallpapers(wallpapers: &[DisplayWallpaper]) -> Self {
        let displays = wallpapers
            .iter()
            .map(|wallpaper| PlasmaWallpaperDisplayState {
                geometry: PlasmaWallpaperGeometry::from(wallpaper.logical_rect),
                image: path_to_image_ref(&wallpaper.path),
            })
            .collect();
        Self {
            version: PLASMA_WALLPAPER_STATE_VERSION,
            updated_at: now_unix(),
            displays,
        }
    }

    /// Finds the still image for a Plasma screen geometry, if present.
    #[must_use]
    pub fn image_for_geometry(&self, x: i32, y: i32, width: u32, height: u32) -> Option<&str> {
        self.displays
            .iter()
            .find(|display| display.geometry.matches(x, y, width, height))
            .map(|display| display.image.as_str())
    }
}

/// Returns the default absolute path for the shared state file.
#[must_use]
pub fn default_plasma_wallpaper_state_path() -> PathBuf {
    plasma_wallpaper_state_dir().join(PLASMA_WALLPAPER_STATE_FILE)
}

/// Returns the default directory that holds Plasma wallpaper IPC files.
#[must_use]
pub fn plasma_wallpaper_state_dir() -> PathBuf {
    directories::ProjectDirs::from("net", "fritztech", "Easel").map_or_else(
        || {
            std::env::temp_dir()
                .join("easel")
                .join("data")
                .join(PLASMA_WALLPAPER_STATE_DIR)
        },
        |dirs| dirs.data_dir().join(PLASMA_WALLPAPER_STATE_DIR),
    )
}

/// Atomically writes `state` to `path` (via a `.part` sibling).
pub fn write_plasma_wallpaper_state(
    path: &Path,
    state: &PlasmaWallpaperState,
) -> Result<(), PlasmaStateError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(state)?;
    let part_name = path.file_name().map_or_else(
        || "active.json.part".into(),
        |name| format!("{}.part", name.to_string_lossy()),
    );
    let part_path = path.with_file_name(part_name);
    fs::write(&part_path, payload)?;
    fs::rename(&part_path, path)?;
    Ok(())
}

/// Reads and parses a Plasma wallpaper state document.
pub fn read_plasma_wallpaper_state(path: &Path) -> Result<PlasmaWallpaperState, PlasmaStateError> {
    let bytes = fs::read(path)?;
    let state: PlasmaWallpaperState = serde_json::from_slice(&bytes)?;
    if state.version == 0 {
        return Err(PlasmaStateError::UnsupportedVersion(state.version));
    }
    Ok(state)
}

/// Writes still-frame state for `wallpapers` to the default IPC path.
pub fn publish_plasma_wallpaper_state(
    wallpapers: &[DisplayWallpaper],
) -> Result<PathBuf, BackendError> {
    let path = default_plasma_wallpaper_state_path();
    let state = PlasmaWallpaperState::from_wallpapers(wallpapers);
    write_plasma_wallpaper_state(&path, &state).map_err(|error| {
        BackendError::Platform(format!(
            "failed to write Plasma wallpaper state {}: {error}",
            path.display()
        ))
    })?;
    Ok(path)
}

/// Stable fingerprint of display geometries (used to skip redundant plugin binds).
#[must_use]
pub fn wallpaper_geometry_fingerprint(wallpapers: &[DisplayWallpaper]) -> String {
    let mut parts: Vec<String> = wallpapers
        .iter()
        .map(|wallpaper| {
            let rect = wallpaper.logical_rect;
            format!("{}:{}:{}:{}", rect.x, rect.y, rect.width, rect.height)
        })
        .collect();
    parts.sort_unstable();
    parts.join("|")
}

fn path_to_image_ref(path: &Path) -> String {
    url::Url::from_file_path(path)
        .map_or_else(|()| path.display().to_string(), |url| url.to_string())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

/// Plasma wallpaper state IPC failure.
#[derive(Debug, Error)]
pub enum PlasmaStateError {
    /// Filesystem error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON encode/decode failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// Unsupported document version.
    #[error("unsupported plasma wallpaper state version {0}")]
    UnsupportedVersion(u32),
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::{DisplayId, LogicalRect};

    fn sample(path: &str, rect: LogicalRect) -> DisplayWallpaper {
        DisplayWallpaper {
            display_id: DisplayId::from_u128(1),
            path: PathBuf::from(path),
            logical_rect: rect,
        }
    }

    #[test]
    fn round_trips_state_file() {
        let root = std::env::temp_dir().join(format!(
            "easel-plasma-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        let path = root.join("active.json");
        let wallpapers = [sample(
            "/tmp/easel-wall.png",
            LogicalRect {
                x: 2560,
                y: 0,
                width: 1920,
                height: 1080,
            },
        )];
        let state = PlasmaWallpaperState::from_wallpapers(&wallpapers);
        write_plasma_wallpaper_state(&path, &state).unwrap();
        let loaded = read_plasma_wallpaper_state(&path).unwrap();
        assert_eq!(loaded.version, PLASMA_WALLPAPER_STATE_VERSION);
        assert_eq!(loaded.displays.len(), 1);
        assert_eq!(
            loaded.image_for_geometry(2560, 0, 1920, 1080),
            Some("file:///tmp/easel-wall.png")
        );
        assert!(loaded.image_for_geometry(0, 0, 800, 600).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fingerprint_is_order_independent() {
        let a = sample(
            "/a.png",
            LogicalRect {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
        );
        let b = sample(
            "/b.png",
            LogicalRect {
                x: 100,
                y: 0,
                width: 100,
                height: 100,
            },
        );
        assert_eq!(
            wallpaper_geometry_fingerprint(&[a.clone(), b.clone()]),
            wallpaper_geometry_fingerprint(&[b, a])
        );
    }
}
