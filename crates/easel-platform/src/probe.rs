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
use crate::gnome::{GnomeBackend, gnome_available};
#[cfg(all(not(windows), not(target_os = "macos")))]
use crate::plasma::{PlasmaBackend, easel_plasma_plugin_id, plasma_available};
#[cfg(all(not(windows), not(target_os = "macos")))]
use crate::plasma_live::PlasmaLiveBackend;
#[cfg(all(not(windows), not(target_os = "macos")))]
use crate::xfce::{XfceBackend, xfce_available};

#[cfg(windows)]
use crate::windows_desktop::WindowsDesktopBackend;

/// Diagnostic result of probing for a motion wallpaper path.
///
/// Preference: continuous Plasma plugin host when available; otherwise the
/// still-backend slideshow path (ADR 0014) which is always available when a
/// still wallpaper backend exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveBackendProbe {
    /// Whether a motion path is available for this session.
    pub supported: bool,
    /// Stable backend key when a path was selected.
    pub backend_id: Option<&'static str>,
    /// Validated motion features (all false when unsupported).
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

/// Probes whether GIF/video motion can be presented in this session.
///
/// - `plasma6-live` when Plasma + Easel plugin are installed (continuous host).
/// - `still-slideshow` when any still wallpaper backend exists (ADR 0014).
/// - unsupported only when no still backend can Apply frames.
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
                reason: "Plasma session with Easel wallpaper plugin; continuous live via shared clock IPC"
                    .into(),
            };
        }
    }

    match select_wallpaper_backend() {
        Ok(backend) => LiveBackendProbe {
            supported: true,
            backend_id: Some("still-slideshow"),
            capabilities: LiveBackendCapabilities {
                animated_images: true,
                video: true,
                per_display_surfaces: backend.capabilities().per_display_images,
                shared_media_clock: true,
                hardware_decode: false,
                pause_when_occluded: false,
            },
            reason: format!(
                "GIF/video as still-backend slideshow via {} (ADR 0014)",
                backend.id()
            ),
        },
        Err(_) => LiveBackendProbe {
            supported: false,
            backend_id: None,
            capabilities: LiveBackendCapabilities::default(),
            reason: "no still wallpaper backend available for motion slideshow".into(),
        },
    }
}

/// Returns a continuous live backend when Plasma + plugin are selected.
///
/// Still-slideshow motion is started by the desktop Apply path (not this trait),
/// because it drives [`WallpaperBackend::apply`] rather than a persistent media
/// surface.
pub fn select_live_wallpaper_backend() -> Result<Box<dyn LiveWallpaperBackend>, BackendError> {
    let probe = probe_live_wallpaper_backend();
    match probe.backend_id {
        #[cfg(all(not(windows), not(target_os = "macos")))]
        Some("plasma6-live") => Ok(Box::new(PlasmaLiveBackend)),
        Some("still-slideshow") => Err(BackendError::LiveWallpaperUnsupported),
        _ => Err(BackendError::LiveWallpaperUnsupported),
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
    fn live_probe_prefers_plasma_or_still_slideshow() {
        let probe = probe_live_wallpaper_backend();
        assert!(!probe.reason.is_empty());
        if !probe.supported {
            assert!(probe.backend_id.is_none());
            return;
        }
        assert!(probe.capabilities.animated_images);
        assert!(probe.capabilities.video);
        match probe.backend_id {
            Some("plasma6-live") => {
                assert!(matches!(
                    select_live_wallpaper_backend().map(|backend| backend.id()),
                    Ok("plasma6-live")
                ));
            }
            Some("still-slideshow") => {
                assert!(matches!(
                    select_live_wallpaper_backend(),
                    Err(BackendError::LiveWallpaperUnsupported)
                ));
            }
            other => panic!("unexpected live backend {other:?}"),
        }
    }
}
