// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Versioned wallpaper profile model.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{AssetId, DisplayId};

/// Current serialized profile schema.
pub const PROFILE_SCHEMA_VERSION: u16 = 1;

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
    /// Optional presentation frame-rate ceiling.
    pub maximum_frames_per_second: Option<u16>,
    /// Pause live playback while the system is using battery power.
    pub pause_on_battery: bool,
    /// Pause live playback while a full-screen application is active.
    pub pause_for_full_screen_app: bool,
}

impl Default for PlaybackPolicy {
    fn default() -> Self {
        Self {
            loop_mode: LoopMode::Loop,
            rate: 1.0,
            maximum_frames_per_second: Some(30),
            pause_on_battery: true,
            pause_for_full_screen_app: true,
        }
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
    /// Current image selection, if fixed.
    pub selected_asset: Option<AssetId>,
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
            selected_asset: None,
            presentation: PresentationMode::Static,
            playback: PlaybackPolicy::default(),
            fit_mode: FitMode::Cover,
            layout_mode: LayoutMode::PhysicalSpan,
            zoom: 1.0,
            focal_x: 0.5,
            focal_y: 0.5,
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
}
