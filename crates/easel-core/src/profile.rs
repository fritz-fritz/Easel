// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Versioned wallpaper profile model.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{AssetId, DisplayGroupId, DisplayId, DynamicStillSetId, RotationQueueId, ScheduleId};

/// Current serialized profile schema.
pub const PROFILE_SCHEMA_VERSION: u16 = 4;

/// Default still-backend Apply poll interval for motion slideshows (ADR 0014).
pub const DEFAULT_STILL_SLIDESHOW_INTERVAL_MS: u64 = 2_000;

/// Fastest still-backend Apply poll interval Easel will honor.
///
/// Wallpaper settings channels (xfconf, gsettings, System Events, COM) are not
/// video pipelines; sub-500 ms thrashing is unsupported outside a continuous host.
pub const MIN_STILL_SLIDESHOW_INTERVAL_MS: u64 = 500;

/// Slowest still-backend Apply poll interval accepted in a profile.
pub const MAX_STILL_SLIDESHOW_INTERVAL_MS: u64 = 60_000;

/// Stable profile identity independent of its display name.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(Uuid);

impl ProfileId {
    /// Creates a new profile identity.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the canonical hyphenated UUID string.
    #[must_use]
    pub fn to_hyphenated_string(self) -> String {
        self.0.hyphenated().to_string()
    }

    /// Parses a hyphenated UUID string.
    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(value.trim())?))
    }
}

impl Default for ProfileId {
    fn default() -> Self {
        Self::new()
    }
}

/// How an image is scaled into its composition region.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FitMode {
    /// Fill the region and crop overflow.
    #[default]
    Cover,
    /// Show the complete image and permit unused space.
    Contain,
    /// Stretch independently on both axes.
    Stretch,
    /// Preserve one source pixel per native output pixel.
    Native,
}

/// Whether composition uses physical layout space or per-display digital fitting.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutMode {
    /// Span one image across physical content rectangles with PPI and bezel correction.
    #[default]
    PhysicalSpan,
    /// Fit the source independently into each display's native pixels.
    Digital,
}

/// How a finite live asset behaves when its playback reaches the end.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopMode {
    /// Restart at the beginning.
    #[default]
    Loop,
    /// Stop on the final frame.
    Once,
}

/// Resource and playback policy for animated images and video.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaybackPolicy {
    /// End-of-stream behavior.
    pub loop_mode: LoopMode,
    /// Playback speed multiplier.
    pub rate: f64,
    /// Optional presentation frame-rate ceiling for **continuous** live hosts
    /// (Plasma plugin / shared [`crate::PlaybackClock`]). Not used to drive
    /// still-backend slideshow Applies — see [`Self::still_slideshow_interval_ms`].
    pub maximum_frames_per_second: Option<u16>,
    /// Minimum wall time between still-backend wallpaper Applies when motion is
    /// presented as a slideshow (ADR 0014). This is a poll interval, not a
    /// video frame period. Ignored by continuous live hosts.
    #[serde(default = "default_still_slideshow_interval_ms")]
    pub still_slideshow_interval_ms: u64,
    /// Pause live playback while the system is using battery power.
    pub pause_on_battery: bool,
    /// Pause live playback while a full-screen application is active.
    pub pause_for_full_screen_app: bool,
}

const fn default_still_slideshow_interval_ms() -> u64 {
    DEFAULT_STILL_SLIDESHOW_INTERVAL_MS
}

impl Default for PlaybackPolicy {
    fn default() -> Self {
        Self {
            loop_mode: LoopMode::Loop,
            rate: 1.0,
            maximum_frames_per_second: Some(30),
            still_slideshow_interval_ms: DEFAULT_STILL_SLIDESHOW_INTERVAL_MS,
            pause_on_battery: true,
            pause_for_full_screen_app: true,
        }
    }
}

impl PlaybackPolicy {
    /// Wall milliseconds between still-slideshow Applies after rate scaling and clamps.
    #[must_use]
    pub fn effective_still_slideshow_interval_ms(self) -> u64 {
        let base = self.still_slideshow_interval_ms.clamp(
            MIN_STILL_SLIDESHOW_INTERVAL_MS,
            MAX_STILL_SLIDESHOW_INTERVAL_MS,
        );
        if !self.rate.is_finite() || self.rate <= 0.0 {
            return base;
        }
        // Interval is capped at 60_000 ms, so f64 mantissa precision is exact here.
        #[allow(clippy::cast_precision_loss)]
        let scaled = (base as f64 / self.rate).round();
        if !scaled.is_finite() || scaled <= 0.0 {
            return base;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_precision_loss,
            clippy::cast_sign_loss
        )]
        let scaled_ms = scaled.min(MAX_STILL_SLIDESHOW_INTERVAL_MS as f64) as u64;
        scaled_ms.clamp(
            MIN_STILL_SLIDESHOW_INTERVAL_MS,
            MAX_STILL_SLIDESHOW_INTERVAL_MS,
        )
    }
}

/// Runtime pipeline selected for a profile.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationMode {
    /// Render and apply one immutable image.
    #[default]
    Static,
    /// Select still frames from time, solar, or schedule rules.
    DynamicStills,
    /// Continuously present an animated image or video on live desktop surfaces.
    LiveMedia,
}

/// Minimal initial profile; composition rules grow behind schema versions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Serialized schema version.
    pub schema_version: u16,
    /// Stable identity.
    pub id: ProfileId,
    /// User-visible name.
    pub name: String,
    /// Displays participating in the initial group.
    pub displays: Vec<DisplayId>,
    /// Optional reusable display group; when set, membership is resolved at apply time.
    #[serde(default)]
    pub display_group_id: Option<DisplayGroupId>,
    /// Current image selection, if fixed.
    pub selected_asset: Option<AssetId>,
    /// Optional rotation queue used when automation advances this profile.
    #[serde(default)]
    pub rotation_queue_id: Option<RotationQueueId>,
    /// Optional schedule that drives unattended applies for this profile.
    #[serde(default)]
    pub schedule_id: Option<ScheduleId>,
    /// Optional dynamic still set used when `presentation` is `DynamicStills`.
    #[serde(default)]
    pub still_set_id: Option<DynamicStillSetId>,
    /// Static, scheduled-still, or persistent live presentation.
    pub presentation: PresentationMode,
    /// Live-media behavior and resource limits.
    pub playback: PlaybackPolicy,
    /// Scaling behavior.
    pub fit_mode: FitMode,
    /// Physical span versus per-display digital fitting.
    #[serde(default)]
    pub layout_mode: LayoutMode,
    /// Zoom multiplier; values below one are rejected.
    pub zoom: f64,
    /// Horizontal focal point from zero through one.
    pub focal_x: f64,
    /// Vertical focal point from zero through one.
    pub focal_y: f64,
}

impl Profile {
    /// Creates a profile with safe defaults.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            id: ProfileId::new(),
            name: name.into(),
            displays: Vec::new(),
            display_group_id: None,
            selected_asset: None,
            rotation_queue_id: None,
            schedule_id: None,
            still_set_id: None,
            presentation: PresentationMode::Static,
            playback: PlaybackPolicy::default(),
            fit_mode: FitMode::Cover,
            layout_mode: LayoutMode::PhysicalSpan,
            zoom: 1.0,
            focal_x: 0.5,
            focal_y: 0.5,
        }
    }

    /// Upgrades older on-disk profiles to the current schema.
    pub fn migrate(mut self) -> Result<Self, ProfileValidationError> {
        match self.schema_version {
            1..=3 => {
                if self.playback.still_slideshow_interval_ms == 0 {
                    self.playback.still_slideshow_interval_ms = DEFAULT_STILL_SLIDESHOW_INTERVAL_MS;
                }
                self.schema_version = PROFILE_SCHEMA_VERSION;
                Ok(self)
            }
            PROFILE_SCHEMA_VERSION => Ok(self),
            other => Err(ProfileValidationError::UnsupportedSchema(other)),
        }
    }

    /// Validates serialized and interactive inputs.
    pub fn validate(&self) -> Result<(), ProfileValidationError> {
        if self.schema_version != PROFILE_SCHEMA_VERSION {
            return Err(ProfileValidationError::UnsupportedSchema(
                self.schema_version,
            ));
        }
        if self.name.trim().is_empty() {
            return Err(ProfileValidationError::EmptyName);
        }
        if !self.zoom.is_finite() || self.zoom < 1.0 {
            return Err(ProfileValidationError::InvalidZoom);
        }
        if !self.playback.rate.is_finite() || self.playback.rate <= 0.0 {
            return Err(ProfileValidationError::InvalidPlaybackRate);
        }
        if self.playback.maximum_frames_per_second == Some(0) {
            return Err(ProfileValidationError::InvalidFrameRateLimit);
        }
        if !(MIN_STILL_SLIDESHOW_INTERVAL_MS..=MAX_STILL_SLIDESHOW_INTERVAL_MS)
            .contains(&self.playback.still_slideshow_interval_ms)
        {
            return Err(ProfileValidationError::InvalidStillSlideshowInterval);
        }
        if !(0.0..=1.0).contains(&self.focal_x) || !(0.0..=1.0).contains(&self.focal_y) {
            return Err(ProfileValidationError::InvalidFocalPoint);
        }
        Ok(())
    }
}

/// Invalid profile model.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProfileValidationError {
    /// No migration exists for the serialized schema.
    #[error("unsupported profile schema version: {0}")]
    UnsupportedSchema(u16),
    /// Profile names must contain visible characters.
    #[error("profile name cannot be empty")]
    EmptyName,
    /// Zoom must be finite and at least one.
    #[error("zoom must be a finite value greater than or equal to one")]
    InvalidZoom,
    /// Playback rate must be finite and greater than zero.
    #[error("playback rate must be a finite value greater than zero")]
    InvalidPlaybackRate,
    /// A configured frame-rate ceiling cannot be zero.
    #[error("playback frame-rate limit must be greater than zero")]
    InvalidFrameRateLimit,
    /// Still-slideshow Apply poll interval is out of the supported range.
    #[error(
        "still slideshow interval must be between {MIN_STILL_SLIDESHOW_INTERVAL_MS} and {MAX_STILL_SLIDESHOW_INTERVAL_MS} milliseconds"
    )]
    InvalidStillSlideshowInterval,
    /// Focal points use normalized zero-to-one coordinates.
    #[error("focal point coordinates must be between zero and one")]
    InvalidFocalPoint,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profile_is_valid() {
        assert_eq!(Profile::new("Home").validate(), Ok(()));
    }

    #[test]
    fn invalid_focal_point_is_rejected() {
        let mut profile = Profile::new("Home");
        profile.focal_x = 1.1;
        assert_eq!(
            profile.validate(),
            Err(ProfileValidationError::InvalidFocalPoint)
        );
    }

    #[test]
    fn zero_live_frame_rate_limit_is_rejected() {
        let mut profile = Profile::new("Home");
        profile.playback.maximum_frames_per_second = Some(0);
        assert_eq!(
            profile.validate(),
            Err(ProfileValidationError::InvalidFrameRateLimit)
        );
    }

    #[test]
    fn still_slideshow_interval_is_clamped_by_effective_helper() {
        let policy = PlaybackPolicy {
            still_slideshow_interval_ms: 50,
            ..PlaybackPolicy::default()
        };
        assert_eq!(
            policy.effective_still_slideshow_interval_ms(),
            MIN_STILL_SLIDESHOW_INTERVAL_MS
        );
        let faster = PlaybackPolicy {
            still_slideshow_interval_ms: 2_000,
            rate: 2.0,
            ..PlaybackPolicy::default()
        };
        assert_eq!(faster.effective_still_slideshow_interval_ms(), 1_000);
    }

    #[test]
    fn still_slideshow_interval_out_of_range_is_rejected() {
        let mut profile = Profile::new("Home");
        profile.playback.still_slideshow_interval_ms = 50;
        assert_eq!(
            profile.validate(),
            Err(ProfileValidationError::InvalidStillSlideshowInterval)
        );
    }

    #[test]
    fn schema_v3_migrates_with_default_slideshow_interval() {
        let mut profile = Profile::new("Home");
        profile.schema_version = 3;
        let migrated = profile.migrate().unwrap();
        assert_eq!(migrated.schema_version, PROFILE_SCHEMA_VERSION);
        assert_eq!(
            migrated.playback.still_slideshow_interval_ms,
            DEFAULT_STILL_SLIDESHOW_INTERVAL_MS
        );
    }
}
