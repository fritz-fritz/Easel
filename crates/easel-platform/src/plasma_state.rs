// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared still-frame and live-session state for the Easel Plasma wallpaper plugin
//! (ADR 0008).
//!
//! Desktop automation writes this file after rendering per-display stills or
//! starting a live session. The Plasma plugin watches it and updates still
//! `Image` sources or live `MediaPlayer` / `AnimatedImage` crops without
//! requiring `PlasmaShell.evaluateScript` on every tick.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use easel_core::{LogicalRect, LoopMode, PlaybackPolicy};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{BackendError, DisplayWallpaper, LiveDisplaySurface};

/// Schema version for [`PlasmaWallpaperState`].
///
/// Version 1 was still-only. Version 2 adds optional [`PlasmaLiveState`].
pub const PLASMA_WALLPAPER_STATE_VERSION: u32 = 2;

/// Oldest schema version this crate still reads.
pub const PLASMA_WALLPAPER_STATE_MIN_VERSION: u32 = 1;

/// Relative directory under the Easel data dir that holds the state file.
pub const PLASMA_WALLPAPER_STATE_DIR: &str = "plasma-wallpaper";

/// File name written by desktop automation and watched by the plugin.
pub const PLASMA_WALLPAPER_STATE_FILE: &str = "active.json";

/// Presentation mode published to the Plasma plugin.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlasmaWallpaperMode {
    /// Per-display still images only.
    #[default]
    Still,
    /// Live media with per-display UV crops; still images remain as posters.
    Live,
}

/// One display's active still frame (poster or static wallpaper).
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

/// Normalized source UV window (`0..=1`) for live crops.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlasmaSourceUv {
    /// Left edge of the source sample window.
    pub x: f64,
    /// Top edge of the source sample window.
    pub y: f64,
    /// Width of the source sample window.
    pub width: f64,
    /// Height of the source sample window.
    pub height: f64,
}

/// Per-display live crop published beside still posters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlasmaLiveDisplayCrop {
    /// Logical compositor rectangle used to match a Plasma containment.
    pub geometry: PlasmaWallpaperGeometry,
    /// UV window into the shared media source.
    pub source_uv: PlasmaSourceUv,
    /// Poster still for this display (startup / failure fallback).
    pub poster: String,
}

/// Live session directive consumed by the Plasma plugin (shared clock via IPC).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlasmaLiveState {
    /// Absolute `file://` URL or path to the animated image / video.
    pub source: String,
    /// `animated_image` or `video`.
    pub media_kind: String,
    /// Source pixel width used to plan crops.
    pub source_width: u32,
    /// Source pixel height used to plan crops.
    pub source_height: u32,
    /// `loop` or `once`.
    pub loop_mode: String,
    /// Playback speed multiplier.
    pub rate: f64,
    /// Optional presentation frame-rate ceiling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_frames_per_second: Option<u16>,
    /// Whether the shared clock is frozen.
    pub paused: bool,
    /// Machine-readable pause reason (`battery`, `session_lock`, …) when paused.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pause_reason: String,
    /// Shared media timeline position in milliseconds.
    pub media_time_ms: u64,
    /// Known media duration when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Per-display UV crops (same order / geometries as still posters).
    pub displays: Vec<PlasmaLiveDisplayCrop>,
}

/// Root document written to [`PLASMA_WALLPAPER_STATE_FILE`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlasmaWallpaperState {
    /// Schema version.
    pub version: u32,
    /// Unix timestamp when the document was written.
    pub updated_at: u64,
    /// Still versus live presentation.
    #[serde(default)]
    pub mode: PlasmaWallpaperMode,
    /// Per-display still frames (always present; posters during live).
    pub displays: Vec<PlasmaWallpaperDisplayState>,
    /// Live session payload when [`Self::mode`] is [`PlasmaWallpaperMode::Live`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<PlasmaLiveState>,
}

/// Playback snapshot published beside live crops.
#[derive(Clone, Debug, PartialEq)]
pub struct PlasmaLiveClockSnapshot {
    /// Whether the shared clock is frozen.
    pub paused: bool,
    /// Machine-readable pause reason when paused.
    pub pause_reason: String,
    /// Shared media timeline position in milliseconds.
    pub media_time_ms: u64,
    /// Known media duration when reported.
    pub duration_ms: Option<u64>,
}

impl PlasmaWallpaperState {
    /// Builds still-only state from renderer output destined for Plasma.
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
            mode: PlasmaWallpaperMode::Still,
            displays,
            live: None,
        }
    }

    /// Builds live state from planned surfaces plus a playback snapshot.
    #[must_use]
    pub fn from_live_surfaces(
        surfaces: &[LiveDisplaySurface],
        source_width: u32,
        source_height: u32,
        media_kind: &str,
        policy: &PlaybackPolicy,
        clock: &PlasmaLiveClockSnapshot,
    ) -> Self {
        let displays = surfaces
            .iter()
            .map(|surface| PlasmaWallpaperDisplayState {
                geometry: PlasmaWallpaperGeometry::from(surface.logical_rect),
                image: path_to_image_ref(&surface.media.poster_frame),
            })
            .collect();
        let live_displays = surfaces
            .iter()
            .map(|surface| PlasmaLiveDisplayCrop {
                geometry: PlasmaWallpaperGeometry::from(surface.logical_rect),
                source_uv: PlasmaSourceUv {
                    x: surface.source_uv.x,
                    y: surface.source_uv.y,
                    width: surface.source_uv.width,
                    height: surface.source_uv.height,
                },
                poster: path_to_image_ref(&surface.media.poster_frame),
            })
            .collect();
        let source = surfaces
            .first()
            .map(|surface| path_to_image_ref(&surface.media.source))
            .unwrap_or_default();
        let loop_mode = match policy.loop_mode {
            LoopMode::Loop => "loop",
            LoopMode::Once => "once",
        };
        Self {
            version: PLASMA_WALLPAPER_STATE_VERSION,
            updated_at: now_unix(),
            mode: PlasmaWallpaperMode::Live,
            displays,
            live: Some(PlasmaLiveState {
                source,
                media_kind: media_kind.to_owned(),
                source_width,
                source_height,
                loop_mode: loop_mode.to_owned(),
                rate: policy.rate,
                maximum_frames_per_second: policy.maximum_frames_per_second,
                paused: clock.paused,
                pause_reason: clock.pause_reason.clone(),
                media_time_ms: clock.media_time_ms,
                duration_ms: clock.duration_ms,
                displays: live_displays,
            }),
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

    /// Finds the live crop for a Plasma screen geometry, if present.
    #[must_use]
    pub fn live_crop_for_geometry(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Option<&PlasmaLiveDisplayCrop> {
        self.live
            .as_ref()?
            .displays
            .iter()
            .find(|display| display.geometry.matches(x, y, width, height))
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
    if state.version < PLASMA_WALLPAPER_STATE_MIN_VERSION
        || state.version > PLASMA_WALLPAPER_STATE_VERSION
    {
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

/// Writes live-session state (with poster stills) to the default IPC path.
pub fn publish_plasma_live_state(state: &PlasmaWallpaperState) -> Result<PathBuf, BackendError> {
    let path = default_plasma_wallpaper_state_path();
    write_plasma_wallpaper_state(&path, state).map_err(|error| {
        BackendError::Platform(format!(
            "failed to write Plasma live wallpaper state {}: {error}",
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

/// Geometry fingerprint for live surfaces (same format as still wallpapers).
#[must_use]
pub fn live_geometry_fingerprint(surfaces: &[LiveDisplaySurface]) -> String {
    let wallpapers: Vec<DisplayWallpaper> = surfaces
        .iter()
        .map(|surface| DisplayWallpaper {
            display_id: surface.display_id,
            path: surface.media.poster_frame.clone(),
            logical_rect: surface.logical_rect,
        })
        .collect();
    wallpaper_geometry_fingerprint(&wallpapers)
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
    use crate::{LiveMediaOutput, SourceUvRect};
    use easel_core::{DisplayId, LogicalRect, LoopMode, PlaybackPolicy};

    fn sample(path: &str, rect: LogicalRect) -> DisplayWallpaper {
        DisplayWallpaper {
            display_id: DisplayId::from_u128(1),
            path: PathBuf::from(path),
            logical_rect: rect,
        }
    }

    fn sample_surface(path: &str, poster: &str, rect: LogicalRect) -> LiveDisplaySurface {
        LiveDisplaySurface {
            display_id: DisplayId::from_u128(1),
            logical_rect: rect,
            media: LiveMediaOutput {
                source: PathBuf::from(path),
                poster_frame: PathBuf::from(poster),
            },
            source_uv: SourceUvRect {
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 1.0,
            },
            source_width: 3840,
            source_height: 1080,
        }
    }

    #[test]
    fn round_trips_still_state_file() {
        let root = std::env::temp_dir().join(format!(
            "easel-plasma-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        let path = root.join("active.json");
        let image_path = root.join("easel-wall.png");
        let expected_image = path_to_image_ref(&image_path);
        let wallpapers = [sample(
            image_path.to_str().expect("temp path is utf-8"),
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
        assert_eq!(loaded.mode, PlasmaWallpaperMode::Still);
        assert!(loaded.live.is_none());
        assert_eq!(loaded.displays.len(), 1);
        assert!(
            expected_image.starts_with("file://"),
            "absolute image path should serialize as a file URL, got {expected_image}"
        );
        assert_eq!(
            loaded.image_for_geometry(2560, 0, 1920, 1080),
            Some(expected_image.as_str())
        );
        assert!(loaded.image_for_geometry(0, 0, 800, 600).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn round_trips_live_state_file() {
        let root = std::env::temp_dir().join(format!(
            "easel-plasma-live-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        let path = root.join("active.json");
        let source = root.join("clip.mp4");
        let poster = root.join("poster.png");
        let surface = sample_surface(
            source.to_str().unwrap(),
            poster.to_str().unwrap(),
            LogicalRect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
        );
        let policy = PlaybackPolicy {
            loop_mode: LoopMode::Loop,
            rate: 1.0,
            maximum_frames_per_second: Some(30),
            pause_on_battery: true,
            pause_for_full_screen_app: true,
        };
        let state = PlasmaWallpaperState::from_live_surfaces(
            &[surface],
            3840,
            1080,
            "video",
            &policy,
            &PlasmaLiveClockSnapshot {
                paused: true,
                pause_reason: "battery".into(),
                media_time_ms: 1_250,
                duration_ms: Some(10_000),
            },
        );
        write_plasma_wallpaper_state(&path, &state).unwrap();
        let loaded = read_plasma_wallpaper_state(&path).unwrap();
        assert_eq!(loaded.mode, PlasmaWallpaperMode::Live);
        let live = loaded.live.as_ref().expect("live payload");
        assert_eq!(live.media_kind, "video");
        assert!(live.paused);
        assert_eq!(live.pause_reason, "battery");
        assert_eq!(live.media_time_ms, 1_250);
        assert!((live.displays[0].source_uv.width - 0.5).abs() < f64::EPSILON);
        assert!(loaded.live_crop_for_geometry(0, 0, 1920, 1080).is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_legacy_v1_still_documents() {
        let root = std::env::temp_dir().join(format!(
            "easel-plasma-v1-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        let path = root.join("active.json");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            &path,
            r#"{"version":1,"updated_at":1,"displays":[{"geometry":{"x":0,"y":0,"width":100,"height":100},"image":"file:///tmp/a.png"}]}"#,
        )
        .unwrap();
        let loaded = read_plasma_wallpaper_state(&path).unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.mode, PlasmaWallpaperMode::Still);
        assert_eq!(loaded.displays.len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_default_state_dir_matches_qml_fallback() {
        // QML uses GenericDataLocation + "/easel/plasma-wallpaper/active.json".
        // directories on Linux lowercases the app name into $XDG_DATA_HOME/easel.
        let dir = plasma_wallpaper_state_dir();
        let dir = dir.to_string_lossy();
        assert!(
            dir.ends_with("/easel/plasma-wallpaper")
                || dir.ends_with("/easel/data/plasma-wallpaper"),
            "unexpected plasma state dir {dir}"
        );
    }

    #[test]
    fn rejects_unsupported_state_version() {
        let root = std::env::temp_dir().join(format!(
            "easel-plasma-state-ver-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        let path = root.join("active.json");
        fs::create_dir_all(&root).unwrap();
        fs::write(&path, r#"{"version":99,"updated_at":1,"displays":[]}"#).unwrap();
        let error = read_plasma_wallpaper_state(&path).unwrap_err();
        assert!(matches!(error, PlasmaStateError::UnsupportedVersion(99)));
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
