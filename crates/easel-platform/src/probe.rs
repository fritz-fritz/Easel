// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wallpaper backend probing and selection.

use crate::desktop_live::DesktopSurfaceLiveBackend;
use crate::{BackendError, LiveBackendCapabilities, LiveWallpaperBackend, WallpaperBackend};

#[cfg(target_os = "macos")]
use crate::macos::MacosBackend;

#[cfg(all(not(windows), not(target_os = "macos")))]
use crate::feh::{FehBackend, feh_available};
#[cfg(all(not(windows), not(target_os = "macos")))]
use crate::gnome::{GnomeBackend, gnome_available};
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
/// Live capabilities must never be inferred from OS name alone. Plasma + plugin is
/// the supported host; otherwise ADR 0014 offers experimental app-owned desktop
/// surfaces that require the Easel desktop process to keep running.
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
        if plasma_available() {
            Ok(Box::new(PlasmaBackend))
        } else if xfce_available() {
            Ok(Box::new(XfceBackend))
        } else if gnome_available() {
            Ok(Box::new(GnomeBackend))
        } else if feh_available() {
            Ok(Box::new(FehBackend))
        } else {
            Err(BackendError::NoBackend)
        }
    }
}

/// Probes whether a live-wallpaper host is available in this session.
///
/// Preference: Plasma + Easel plugin (`plasma6-live`, supported). Otherwise the
/// experimental app-owned desktop-surface host (`desktop-surface-live`, ADR 0014).
#[must_use]
pub fn probe_live_wallpaper_backend() -> LiveBackendProbe {
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        if plasma_available() && easel_plasma_plugin_id().is_some() {
            let backend = PlasmaLiveBackend;
            return LiveBackendProbe {
                supported: true,
                backend_id: Some(backend.id()),
                capabilities: backend.capabilities(),
                reason:
                    "Plasma session with Easel wallpaper plugin; live playback via shared clock IPC"
                        .into(),
            };
        }
    }

    desktop_surface_live_probe()
}

fn desktop_surface_live_probe() -> LiveBackendProbe {
    let backend = DesktopSurfaceLiveBackend;
    LiveBackendProbe {
        supported: true,
        backend_id: Some(backend.id()),
        capabilities: backend.capabilities(),
        reason: "experimental app-owned desktop surfaces (ADR 0014); requires Easel to keep running; icon stacking is DE-dependent".into(),
    }
}

/// Returns a live backend when the current session has a selected host.
///
/// Prefer [`probe_live_wallpaper_backend`] for diagnostics. Callers should still
/// seed poster frames through the still backend before `start`.
pub fn select_live_wallpaper_backend() -> Result<Box<dyn LiveWallpaperBackend>, BackendError> {
    let probe = probe_live_wallpaper_backend();
    if !probe.supported {
        return Err(BackendError::LiveWallpaperUnsupported);
    }

    match probe.backend_id {
        #[cfg(all(not(windows), not(target_os = "macos")))]
        Some("plasma6-live") => Ok(Box::new(PlasmaLiveBackend)),
        Some("desktop-surface-live") => Ok(Box::new(DesktopSurfaceLiveBackend)),
        other => Err(BackendError::Platform(format!(
            "live probe selected unknown backend {other:?}"
        ))),
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
                #[cfg(all(not(windows), not(target_os = "macos")))]
                {
                    assert!(
                        matches!(
                            backend.id(),
                            "plasma6" | "xfce-xfconf" | "gnome-gsettings" | "x11-feh"
                        ),
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
    fn live_probe_selects_plasma_or_desktop_surface() {
        let probe = probe_live_wallpaper_backend();
        assert!(probe.supported);
        assert!(!probe.reason.is_empty());
        assert!(probe.capabilities.animated_images);
        assert!(probe.capabilities.video);
        assert!(probe.capabilities.shared_media_clock);
        let id = select_live_wallpaper_backend().expect("live backend").id();
        assert!(
            matches!(id, "plasma6-live" | "desktop-surface-live"),
            "unexpected live backend {id}"
        );
        assert_eq!(probe.backend_id, Some(id));
    }
}
