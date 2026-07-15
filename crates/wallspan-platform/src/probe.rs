//! Wallpaper backend probing and selection.

use crate::plasma::{plasma_available, PlasmaBackend};
use crate::{BackendError, WallpaperBackend};

#[cfg(windows)]
use crate::windows_desktop::WindowsDesktopBackend;

/// Probes the current session and returns the preferred still-wallpaper backend.
pub fn select_wallpaper_backend() -> Result<Box<dyn WallpaperBackend>, BackendError> {
    #[cfg(windows)]
    {
        return Ok(Box::new(WindowsDesktopBackend));
    }

    #[cfg(not(windows))]
    {
        if plasma_available() {
            return Ok(Box::new(PlasmaBackend));
        }
        Err(BackendError::NoBackend)
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
}
