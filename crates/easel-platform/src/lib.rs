// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Capability-reporting operating-system wallpaper backend contracts and adapters.

#![cfg_attr(not(windows), forbid(unsafe_code))]

mod appearance;
#[cfg(all(not(windows), not(target_os = "macos")))]
mod feh;
#[cfg(all(not(windows), not(target_os = "macos")))]
mod gnome;
mod live_policy;
#[cfg(target_os = "macos")]
mod macos;
mod plasma;
#[cfg(all(not(windows), not(target_os = "macos")))]
mod plasma_live;
mod plasma_state;
mod probe;
#[cfg(windows)]
mod windows_desktop;
#[cfg(all(not(windows), not(target_os = "macos")))]
mod xfce;
#[cfg(all(not(windows), not(target_os = "macos")))]
mod xrandr;

use std::path::{Path, PathBuf};

use easel_core::{DisplayId, LogicalRect, PlaybackPolicy};
use thiserror::Error;

pub use appearance::system_appearance;
#[cfg(all(not(windows), not(target_os = "macos")))]
pub use feh::{FehBackend, feh_available};
#[cfg(all(not(windows), not(target_os = "macos")))]
pub use gnome::{GnomeBackend, gnome_available};
pub use live_policy::{
    LivePauseReason, LivePolicySensors, pause_reason_for, probe_live_policy_sensors,
};
#[cfg(target_os = "macos")]
pub use macos::MacosBackend;
pub use plasma::{
    EASEL_PLASMA_WALLPAPER_PLUGIN_ID, PlasmaBackend, build_plasma_native_dynamic_script,
    build_plasma_wallpaper_script, easel_plasma_plugin_id, escape_js_string,
    plasma_dynamic_plugin_id, preferred_still_wallpaper_plugin_id,
};
#[cfg(all(not(windows), not(target_os = "macos")))]
pub use plasma_live::PlasmaLiveBackend;
pub use plasma_state::{
    PLASMA_WALLPAPER_STATE_DIR, PLASMA_WALLPAPER_STATE_FILE, PLASMA_WALLPAPER_STATE_VERSION,
    PlasmaLiveClockSnapshot, PlasmaLiveDisplayCrop, PlasmaLiveState, PlasmaSourceUv,
    PlasmaStateError, PlasmaWallpaperDisplayState, PlasmaWallpaperGeometry, PlasmaWallpaperMode,
    PlasmaWallpaperState, default_plasma_wallpaper_state_path, live_geometry_fingerprint,
    plasma_wallpaper_state_dir, publish_plasma_live_state, publish_plasma_wallpaper_state,
    read_plasma_wallpaper_state, wallpaper_geometry_fingerprint, write_plasma_wallpaper_state,
};
pub use probe::{
    LiveBackendProbe, probe_live_wallpaper_backend, select_live_wallpaper_backend,
    select_wallpaper_backend,
};
#[cfg(windows)]
pub use windows_desktop::WindowsDesktopBackend;
#[cfg(all(not(windows), not(target_os = "macos")))]
pub use xfce::{XfceBackend, xfce_available};

/// Features exposed by the selected desktop backend.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BackendCapabilities {
    /// Can assign one native image to each display.
    pub per_display_images: bool,
    /// Can assign one combined virtual-desktop image.
    pub virtual_desktop_image: bool,
    /// Can maintain wallpaper state per activity.
    pub activities: bool,
    /// Can maintain wallpaper state per workspace.
    pub workspaces: bool,
    /// Can set the lock-screen image through an authorized API.
    pub lock_screen: bool,
    /// Can cross-fade still replacements without a live wallpaper host.
    pub cross_fade: bool,
    /// Can host a native dynamic still package (Apple/Plasma HEIC) without Easel polling.
    pub native_dynamic_bundle: bool,
}

/// One completed still image ready for a specific output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayWallpaper {
    /// Stable Easel display identity.
    pub display_id: DisplayId,
    /// Absolute path to a completed PNG/JPEG wallpaper file.
    pub path: PathBuf,
    /// Logical compositor rectangle used to match the platform output.
    pub logical_rect: LogicalRect,
}

/// Completed renderer output passed to a backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WallpaperOutput {
    /// One combined image.
    VirtualDesktop(PathBuf),
    /// Native still image for each display.
    PerDisplay(Vec<DisplayWallpaper>),
    /// Native dynamic HEIC/AVIF package per display (OS evaluates the schedule).
    NativeDynamic(Vec<DisplayWallpaper>),
}

/// Runtime features exposed by a persistent live-wallpaper host.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LiveBackendCapabilities {
    /// Can continuously present animated image containers.
    pub animated_images: bool,
    /// Can continuously present video containers.
    pub video: bool,
    /// Can attach a distinct live surface to each participating display.
    pub per_display_surfaces: bool,
    /// Can drive all display surfaces from one synchronized media clock.
    pub shared_media_clock: bool,
    /// Can request a hardware-accelerated decoding path.
    pub hardware_decode: bool,
    /// Can suspend work when the live surface is fully occluded.
    pub pause_when_occluded: bool,
}

/// Normalized source UV window (`0..=1`) shared by live hosts and Plasma IPC.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceUvRect {
    /// Left edge of the source sample window.
    pub x: f64,
    /// Top edge of the source sample window.
    pub y: f64,
    /// Width of the source sample window.
    pub width: f64,
    /// Height of the source sample window.
    pub height: f64,
}

/// One playable source and its mandatory safe static fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveMediaOutput {
    /// Local animated-image or video source.
    pub source: PathBuf,
    /// Completed still image used during startup, failure, or unsupported sessions.
    pub poster_frame: PathBuf,
}

/// Per-display live surface with a crop into the shared media timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveDisplaySurface {
    /// Stable Easel display identity.
    pub display_id: DisplayId,
    /// Logical compositor rectangle used to match the platform output.
    pub logical_rect: LogicalRect,
    /// Source media and poster for this surface (typically shared across displays).
    pub media: LiveMediaOutput,
    /// UV window into the shared source (from [`easel_render::plan_live_crops`]).
    pub source_uv: SourceUvRect,
    /// Oriented source width used when planning crops.
    pub source_width: u32,
    /// Oriented source height used when planning crops.
    pub source_height: u32,
}

/// Prepared live content passed to a platform host.
#[derive(Clone, Debug, PartialEq)]
pub enum LiveWallpaperOutput {
    /// One media composition spans the virtual desktop.
    VirtualDesktop(LiveMediaOutput),
    /// Independently cropped media for each display, sharing one logical clock.
    PerDisplay(Vec<LiveDisplaySurface>),
}

/// OS/desktop adapter selected after explicit probing.
pub trait WallpaperBackend: Send + Sync {
    /// Stable backend key used in diagnostics.
    fn id(&self) -> &'static str;

    /// Features that can be used safely on the current session.
    fn capabilities(&self) -> BackendCapabilities;

    /// Applies only completed output files.
    fn apply(&self, output: &WallpaperOutput) -> Result<(), BackendError>;

    /// Validates that an output path is readable before platform mutation.
    fn validate_output_path(&self, path: &Path) -> Result<(), BackendError> {
        if !path.is_file() {
            return Err(BackendError::MissingOutput(path.to_path_buf()));
        }
        Ok(())
    }
}

/// Running live-wallpaper session controlled by power and application policy.
pub trait LiveWallpaperSession: Send {
    /// Pauses decoding and presentation without discarding the current frame.
    fn pause(&mut self) -> Result<(), BackendError>;

    /// Resumes a paused session from the shared logical clock.
    fn resume(&mut self) -> Result<(), BackendError>;

    /// Stops playback and releases every desktop surface.
    fn stop(self: Box<Self>) -> Result<(), BackendError>;
}

/// Platform integration that owns persistent surfaces below desktop icons.
pub trait LiveWallpaperBackend: Send + Sync {
    /// Stable backend key used in diagnostics.
    fn id(&self) -> &'static str;

    /// Features validated for the current desktop session.
    fn capabilities(&self) -> LiveBackendCapabilities;

    /// Starts silent playback. Implementations must never route source audio.
    fn start(
        &self,
        output: &LiveWallpaperOutput,
        policy: PlaybackPolicy,
    ) -> Result<Box<dyn LiveWallpaperSession>, BackendError>;
}

/// Backend probing or mutation failure.
#[derive(Debug, Error)]
pub enum BackendError {
    /// Rendered output disappeared before it could be applied.
    #[error("wallpaper output does not exist: {0}")]
    MissingOutput(PathBuf),
    /// Requested output shape is not supported by the backend.
    #[error("backend does not support the requested output shape")]
    UnsupportedOutput,
    /// No supported still-wallpaper backend is available in this session.
    #[error("no supported wallpaper backend is available")]
    NoBackend,
    /// Current desktop session has no safe live-surface integration.
    #[error("live wallpapers are unsupported in the current desktop session")]
    LiveWallpaperUnsupported,
    /// The media decoder does not support the selected container or codec.
    #[error("media format is unsupported: {0}")]
    UnsupportedMedia(String),
    /// Platform API returned a failure.
    #[error("platform operation failed: {0}")]
    Platform(String),
}
