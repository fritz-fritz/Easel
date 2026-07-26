// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wallpaper backend probing and selection.

use crate::{BackendError, LiveBackendCapabilities, LiveWallpaperBackend, WallpaperBackend};

#[cfg(target_os = "macos")]
use crate::macos::MacosBackend;

#[cfg(all(not(windows), not(target_os = "macos")))]
use crate::plasma::{PlasmaBackend, plasma_available};

#[cfg(windows)]
use crate::windows_desktop::WindowsDesktopBackend;

/// Diagnostic result of probing for a persistent live-wallpaper host.
///
/// Live capabilities must never be inferred from OS name alone. Until a validated
/// host exists for the current session, [`Self::supported`] is false and Apply
/// should use poster-frame fallback through the still [`WallpaperBackend`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveBackendProbe {
    /// Whether a live host passed capability gates for this session.
    pub supported: bool,
    /// Stable backend key when a host was selected.
    pub backend_id: Option<&'static str>,
    /// Validated live features (all false when unsupported).
    pub capabilities: LiveBackendCapabilities,
    /// Human-readable evidence for UI diagnostics and status lines.
    pub reason: String,
}

/// Probes the current session and returns the preferred still-wallpaper backend.
pub fn select_wallpaper_backend() -> Result<Box<dyn WallpaperBackend>, BackendError> {
    #[cfg(windows)]
    {
        Ok(Box::new(WindowsDesktopBackend))
    }

    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(MacosBackend))
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        if plasma_available() {
            Ok(Box::new(PlasmaBackend))
        } else {
            Err(BackendError::NoBackend)
        }
    }
}

/// Probes whether a persistent live-wallpaper host is available in this session.
///
/// Stage 6.6: no live host is gated as supported yet. Plasma still-frame IPC and
/// Qt Multimedia preview exist; plugin live playback and Win/macOS hosts follow.
#[must_use]
pub fn probe_live_wallpaper_backend() -> LiveBackendProbe {
    LiveBackendProbe {
        supported: false,
        backend_id: None,
        capabilities: LiveBackendCapabilities::default(),
        reason: live_unsupported_reason(),
    }
}

/// Returns a live backend only when the current session has a validated host.
///
/// Until Plasma plugin live playback (and later Win/macOS spikes) land, this always
/// returns [`BackendError::LiveWallpaperUnsupported`]. Callers must apply the
/// poster frame through [`select_wallpaper_backend`] instead.
pub fn select_live_wallpaper_backend() -> Result<Box<dyn LiveWallpaperBackend>, BackendError> {
    let _ = probe_live_wallpaper_backend();
    Err(BackendError::LiveWallpaperUnsupported)
}

fn live_unsupported_reason() -> String {
    // Keep this focused on why live is unsupported; callers describe poster fallback.
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        if plasma_available() {
            "Plasma session detected; Easel wallpaper plugin live playback is not enabled yet"
                .into()
        } else {
            "no validated live wallpaper host in this desktop session".into()
        }
    }
    #[cfg(windows)]
    {
        "Windows live wallpaper host is experimental and not enabled".into()
    }
    #[cfg(target_os = "macos")]
    {
        "macOS live wallpaper host is experimental and not enabled".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_returns_concrete_backend_or_no_backend() {
        match select_wallpaper_backend() {
            Ok(backend) => {
                assert!(!backend.id().is_empty());
                assert!(backend.capabilities().per_display_images);
            }
            Err(BackendError::NoBackend) => {}
            Err(other) => panic!("unexpected probe error: {other}"),
        }
    }

    #[test]
    fn live_probe_is_unsupported_until_host_lands() {
        let probe = probe_live_wallpaper_backend();
        assert!(!probe.supported);
        assert!(probe.backend_id.is_none());
        assert!(!probe.capabilities.animated_images);
        assert!(!probe.capabilities.video);
        assert!(!probe.capabilities.shared_media_clock);
        assert!(!probe.reason.is_empty());
        assert!(
            matches!(
                select_live_wallpaper_backend(),
                Err(BackendError::LiveWallpaperUnsupported)
            ),
            "live select must stay gated"
        );
    }
}
