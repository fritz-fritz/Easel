// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Local and remote media asset provenance.

use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

/// Stable identity within the local Easel library.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(Uuid);

impl AssetId {
    /// Creates a new asset identity.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AssetId {
    fn default() -> Self {
        Self::new()
    }
}

/// Provider-specific identity retained without reinterpretation.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ProviderAssetId {
    /// Stable provider key such as `openverse`.
    pub provider: String,
    /// Provider's opaque asset identifier.
    pub asset_id: String,
}

/// Where media bytes originate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum AssetLocation {
    /// User-controlled local file.
    Local {
        /// Absolute path serialized as text at the adapter boundary.
        path: String,
    },
    /// Provider-controlled remote asset.
    Remote {
        /// Canonical page describing the work.
        canonical_work_url: Url,
        /// Preview URL suitable for browsing.
        preview_url: Url,
        /// Provider-authorized acquisition URL.
        acquisition_url: Url,
    },
}

/// License evidence attached to a remote work.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetLicense {
    /// Normalized SPDX-like identifier when known.
    pub identifier: String,
    /// Human-readable license version.
    pub version: Option<String>,
    /// Canonical license terms.
    pub url: Url,
}

/// Attribution that must travel with the asset.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Attribution {
    /// Creator display name.
    pub creator_name: String,
    /// Creator's canonical provider/source page.
    pub creator_url: Option<Url>,
    /// Ready-to-display attribution statement.
    pub text: String,
}

/// Native media dimensions in pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MediaDimensions {
    /// Native width reported by the decoder or provider.
    pub width: u32,
    /// Native height reported by the decoder or provider.
    pub height: u32,
}

/// Exact rational frame rate without floating-point serialization drift.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FrameRate {
    /// Frames in one rate interval.
    pub numerator: u32,
    /// Seconds denominator for the rate interval.
    pub denominator: u32,
}

/// Decoder-visible technical metadata used to choose a presentation pipeline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MediaMetadata {
    /// One still image.
    StillImage {
        /// Decoded pixel dimensions.
        dimensions: MediaDimensions,
    },
    /// An animated image container such as GIF or animated WebP.
    AnimatedImage {
        /// Decoded pixel dimensions.
        dimensions: MediaDimensions,
        /// Total duration when the container reports one.
        duration_ms: Option<u64>,
        /// Decoded frame count when known without a full scan.
        frame_count: Option<u32>,
    },
    /// A video container decoded by the live presentation pipeline.
    Video {
        /// Decoded pixel dimensions.
        dimensions: MediaDimensions,
        /// Total duration when reported by the container.
        duration_ms: Option<u64>,
        /// Nominal frame rate when reported by the container.
        frame_rate: Option<FrameRate>,
        /// Container name used for diagnostics, such as `matroska`.
        container: Option<String>,
        /// Video codec name used for diagnostics, such as `av1`.
        video_codec: Option<String>,
        /// Whether an audio stream exists. Easel never presents audio.
        has_audio: bool,
    },
}

impl MediaMetadata {
    /// Returns the decoded pixel dimensions for suitability checks.
    #[must_use]
    pub const fn dimensions(&self) -> MediaDimensions {
        match self {
            Self::StillImage { dimensions }
            | Self::AnimatedImage { dimensions, .. }
            | Self::Video { dimensions, .. } => *dimensions,
        }
    }

    /// Returns whether presentation requires a persistent live surface.
    #[must_use]
    pub const fn requires_live_surface(&self) -> bool {
        matches!(self, Self::AnimatedImage { .. } | Self::Video { .. })
    }
}

/// Media plus immutable provenance used by discovery, selection, and history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MediaAsset {
    /// Local library identity.
    pub id: AssetId,
    /// Optional provider identity.
    pub provider_id: Option<ProviderAssetId>,
    /// User/provider-facing title.
    pub title: Option<String>,
    /// Decoder-visible media type and technical metadata.
    pub media: MediaMetadata,
    /// Media byte location.
    pub location: AssetLocation,
    /// License evidence when applicable.
    pub license: Option<AssetLicense>,
    /// Required or recommended attribution.
    pub attribution: Option<Attribution>,
}
