// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wallpaper backend probing and selection.

use crate::{
    BackendCapabilities, BackendError, LiveBackendCapabilities, LiveWallpaperBackend,
    WallpaperBackend,
};

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

/// How dynamic stills are applied once a still [`WallpaperBackend`] is available.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicStillsHost {
    /// No still backend is available in this session.
    Unavailable,
    /// Scheduler evaluates frames and applies PNG/JPEG crops through the still backend.
    StillPoller,
    /// Prefer OS-hosted native packages when the still set allows; still poller remains
    /// the fallback (dense Plasma solar, encode failures, etc.).
    NativeBundleWithPollerFallback,
}

/// Diagnostic result of probing the still wallpaper backend (and dynamic-still path).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WallpaperBackendProbe {
    /// Whether a still backend was selected.
    pub available: bool,
    /// Stable backend key when selected.
    pub backend_id: Option<&'static str>,
    /// Still-backend capability flags (defaults when unavailable).
    pub capabilities: BackendCapabilities,
    /// Dynamic-still apply strategy for this session.
    pub dynamic_stills: DynamicStillsHost,
    /// Human-readable evidence for UI diagnostics and status lines.
    pub reason: String,
}

/// PRODUCT four-mode support report for the active desktop session.
///
/// Static and dynamic stills require a still backend. Animated-image and video require a
/// validated motion path (`plasma6-live` or `still-slideshow`; never inferred from OS name).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationSupport {
    /// Still apply through the selected wallpaper backend (per-display and/or virtual-desktop).
    pub static_stills: bool,
    /// Dynamic still sets via poller and/or native package host.
    pub dynamic_stills: bool,
    /// Animated-image motion via continuous host or still-backend slideshow.
    pub animated_images: bool,
    /// Silent video motion via continuous host or still-backend slideshow.
    pub video: bool,
    /// Selected still backend id when available.
    pub still_backend_id: Option<&'static str>,
    /// Selected live backend id when available.
    pub live_backend_id: Option<&'static str>,
    /// Dynamic-still host strategy.
    pub dynamic_host: DynamicStillsHost,
    /// Still / dynamic probe evidence.
    pub still_reason: String,
    /// Live probe evidence.
    pub live_reason: String,
}

impl DynamicStillsHost {
    /// Short label for CLI / Compose status lines.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::StillPoller => "still poller",
            Self::NativeBundleWithPollerFallback => "native package + still-poller fallback",
        }
    }
}

/// Probes the current session and returns still-backend diagnostics (including dynamic stills).
#[must_use]
pub fn probe_wallpaper_backend() -> WallpaperBackendProbe {
    match select_wallpaper_backend() {
        Ok(backend) => {
            let capabilities = backend.capabilities();
            let dynamic_stills = if capabilities.native_dynamic_bundle {
                DynamicStillsHost::NativeBundleWithPollerFallback
            } else {
                DynamicStillsHost::StillPoller
            };
            let reason = match dynamic_stills {
                DynamicStillsHost::NativeBundleWithPollerFallback => format!(
                    "still backend {}; dynamic stills prefer native packages with still-poller fallback",
                    backend.id()
                ),
                DynamicStillsHost::StillPoller => format!(
                    "still backend {}; dynamic stills use still-frame poller (no public native dynamic host)",
                    backend.id()
                ),
                DynamicStillsHost::Unavailable => unreachable!("selected backend is available"),
            };
            WallpaperBackendProbe {
                available: true,
                backend_id: Some(backend.id()),
                capabilities,
                dynamic_stills,
                reason,
            }
        }
        Err(_) => WallpaperBackendProbe {
            available: false,
            backend_id: None,
            capabilities: BackendCapabilities::default(),
            dynamic_stills: DynamicStillsHost::Unavailable,
            reason: "no supported wallpaper backend is available".into(),
        },
    }
}

/// Reports static / dynamic-still / animated / video support for the active session.
#[must_use]
pub fn probe_presentation_support() -> PresentationSupport {
    let still = probe_wallpaper_backend();
    let live = probe_live_wallpaper_backend();
    PresentationSupport {
        static_stills: still.available && still_output_supported(still.capabilities),
        dynamic_stills: still.dynamic_stills != DynamicStillsHost::Unavailable,
        animated_images: live.supported && live.capabilities.animated_images,
        video: live.supported && live.capabilities.video,
        still_backend_id: still.backend_id,
        live_backend_id: live.backend_id,
        dynamic_host: still.dynamic_stills,
        still_reason: still.reason,
        live_reason: live.reason,
    }
}

/// True when the backend can apply at least one still output shape.
#[must_use]
fn still_output_supported(capabilities: BackendCapabilities) -> bool {
    capabilities.per_display_images || capabilities.virtual_desktop_image
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
    fn wallpaper_probe_reports_dynamic_stills_for_every_still_backend() {
        let probe = probe_wallpaper_backend();
        if !probe.available {
            assert_eq!(probe.dynamic_stills, DynamicStillsHost::Unavailable);
            return;
        }
        assert!(probe.backend_id.is_some());
        assert!(probe.capabilities.per_display_images);
        assert_ne!(probe.dynamic_stills, DynamicStillsHost::Unavailable);
        if probe.capabilities.native_dynamic_bundle {
            assert_eq!(
                probe.dynamic_stills,
                DynamicStillsHost::NativeBundleWithPollerFallback
            );
        } else {
            assert_eq!(probe.dynamic_stills, DynamicStillsHost::StillPoller);
        }
        let support = probe_presentation_support();
        assert!(support.static_stills);
        assert!(support.dynamic_stills);
        assert_eq!(support.still_backend_id, probe.backend_id);
        assert_eq!(support.dynamic_host, probe.dynamic_stills);
        let live = probe_live_wallpaper_backend();
        assert_eq!(
            support.animated_images,
            live.supported && live.capabilities.animated_images
        );
        assert_eq!(support.video, live.supported && live.capabilities.video);
    }

    #[test]
    fn static_stills_accepts_virtual_desktop_only_backends() {
        let caps = BackendCapabilities {
            per_display_images: false,
            virtual_desktop_image: true,
            ..BackendCapabilities::default()
        };
        assert!(still_output_supported(caps));
        let neither = BackendCapabilities::default();
        assert!(!still_output_supported(neither));
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
