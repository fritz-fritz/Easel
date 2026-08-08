// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wallpaper backend probing and selection.

use crate::{BackendError, LiveBackendCapabilities, LiveWallpaperBackend, WallpaperBackend};

#[cfg(target_os = "macos")]
use crate::macos::MacosBackend;

#[cfg(all(not(windows), not(target_os = "macos")))]
use crate::feh::{FehBackend, feh_available};
#[cfg(all(not(windows), not(target_os = "macos")))]
use crate::plasma::{PlasmaBackend, easel_plasma_plugin_id, plasma_available};
#[cfg(all(not(windows), not(target_os = "macos")))]
use crate::plasma_live::PlasmaLiveBackend;
#[cfg(all(not(windows), not(target_os = "macos")))]
use crate::xfce::{XfceBackend, xfce_available};

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
        // Preference: desktop-native channels that persist settings, then generic X.
        // GNOME gsettings remains Stage 7.2 (no stable public per-monitor still API).
        if plasma_available() {
            Ok(Box::new(PlasmaBackend))
        } else if xfce_available() {
            Ok(Box::new(XfceBackend))
        } else if feh_available() {
            Ok(Box::new(FehBackend))
        } else {
            Err(BackendError::NoBackend)
        }
    }
}

/// Probes whether a persistent live-wallpaper host is available in this session.
///
/// Plasma: supported when the Easel wallpaper plugin package is installed
/// (`net.fritztech.easel.wallpaper`). Windows/macOS remain unsupported after the
/// Stage 6 feasibility spikes (public wallpaper APIs are still-image only).
#[must_use]
pub fn probe_live_wallpaper_backend() -> LiveBackendProbe {
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        if plasma_available() {
            if easel_plasma_plugin_id().is_some() {
                let backend = PlasmaLiveBackend;
                return LiveBackendProbe {
                    supported: true,
                    backend_id: Some(backend.id()),
                    capabilities: backend.capabilities(),
                    reason: "Plasma session with Easel wallpaper plugin; live playback via shared clock IPC"
                        .into(),
                };
            }
            return LiveBackendProbe {
                supported: false,
                backend_id: None,
                capabilities: LiveBackendCapabilities::default(),
                reason:
                    "Plasma session detected; install the Easel wallpaper plugin for live playback"
                        .into(),
            };
        }
        LiveBackendProbe {
            supported: false,
            backend_id: None,
            capabilities: LiveBackendCapabilities::default(),
            reason: "no validated live wallpaper host in this desktop session".into(),
        }
    }

    #[cfg(windows)]
    {
        LiveBackendProbe {
            supported: false,
            backend_id: None,
            capabilities: LiveBackendCapabilities::default(),
            reason: windows_live_spike_reason().into(),
        }
    }

    #[cfg(target_os = "macos")]
    {
        LiveBackendProbe {
            supported: false,
            backend_id: None,
            capabilities: LiveBackendCapabilities::default(),
            reason: macos_live_spike_reason().into(),
        }
    }
}

/// Returns a live backend only when the current session has a validated host.
///
/// Callers must apply the poster frame through [`select_wallpaper_backend`] when
/// this returns [`BackendError::LiveWallpaperUnsupported`].
pub fn select_live_wallpaper_backend() -> Result<Box<dyn LiveWallpaperBackend>, BackendError> {
    let probe = probe_live_wallpaper_backend();
    if !probe.supported {
        return Err(BackendError::LiveWallpaperUnsupported);
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let _ = probe;
        Ok(Box::new(PlasmaLiveBackend))
    }

    #[cfg(any(windows, target_os = "macos"))]
    {
        let _ = probe;
        Err(BackendError::LiveWallpaperUnsupported)
    }
}

#[cfg(windows)]
fn windows_live_spike_reason() -> &'static str {
    // ADR 0010: IDesktopWallpaper / SystemParametersInfo accept still images only.
    "Windows live wallpaper unsupported — IDesktopWallpaper has no public video surface (ADR 0010); poster fallback"
}

#[cfg(target_os = "macos")]
fn macos_live_spike_reason() -> &'static str {
    // ADR 0010: NSWorkspace setDesktopImageURL is still-image oriented.
    "macOS live wallpaper unsupported — setDesktopImageURL is still-image only (ADR 0010); poster fallback"
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
                #[cfg(all(not(windows), not(target_os = "macos")))]
                {
                    assert!(
                        matches!(backend.id(), "plasma6" | "xfce-xfconf" | "x11-feh"),
                        "unexpected linux still backend {}",
                        backend.id()
                    );
                }
            }
            Err(BackendError::NoBackend) => {}
            Err(other) => panic!("unexpected probe error: {other}"),
        }
    }

    #[test]
    fn live_probe_is_honest_about_session() {
        let probe = probe_live_wallpaper_backend();
        assert!(!probe.reason.is_empty());
        if probe.supported {
            assert_eq!(probe.backend_id, Some("plasma6-live"));
            assert!(probe.capabilities.animated_images);
            assert!(probe.capabilities.video);
            assert!(probe.capabilities.shared_media_clock);
            assert!(matches!(
                select_live_wallpaper_backend().map(|backend| backend.id()),
                Ok("plasma6-live")
            ));
        } else {
            assert!(probe.backend_id.is_none());
            assert!(!probe.capabilities.animated_images);
            assert!(matches!(
                select_live_wallpaper_backend(),
                Err(BackendError::LiveWallpaperUnsupported)
            ));
        }
    }
}
