// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! macOS wallpaper backend. Dynamic HEIC packages are handed to the OS.

use std::path::Path;
use std::process::Command;

use crate::{BackendCapabilities, BackendError, WallpaperBackend, WallpaperOutput};

/// macOS desktop wallpaper backend via AppleScript / System Events.
#[derive(Clone, Copy, Debug, Default)]
pub struct MacosBackend;

impl WallpaperBackend for MacosBackend {
    fn id(&self) -> &'static str {
        "macos"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            per_display_images: true,
            virtual_desktop_image: true,
            activities: false,
            workspaces: false,
            lock_screen: false,
            cross_fade: true,
            native_dynamic_bundle: true,
        }
    }

    fn apply(&self, output: &WallpaperOutput) -> Result<(), BackendError> {
        match output {
            WallpaperOutput::VirtualDesktop(path) => {
                self.validate_output_path(path)?;
                set_desktop_picture(path)
            }
            WallpaperOutput::PerDisplay(displays) | WallpaperOutput::NativeDynamic(displays) => {
                for wallpaper in displays {
                    self.validate_output_path(&wallpaper.path)?;
                }
                // System Events enumerates desktops in display order; we assign by index.
                for (index, wallpaper) in displays.iter().enumerate() {
                    set_desktop_picture_at(index + 1, &wallpaper.path)?;
                }
                Ok(())
            }
        }
    }
}

fn set_desktop_picture(path: &Path) -> Result<(), BackendError> {
    let posix = path
        .to_str()
        .ok_or_else(|| BackendError::Platform("wallpaper path is not valid UTF-8".into()))?;
    // VirtualDesktop is one combined image for every desktop/space the OS enumerates.
    let script = format!(
        r#"tell application "System Events"
  try
    set desktopCount to count of desktops
    repeat with desktopIndex from 1 to desktopCount
      set picture of desktop desktopIndex to POSIX file "{posix}"
    end repeat
  end try
end tell"#
    );
    let status = Command::new("osascript")
        .args(["-e", &script])
        .status()
        .map_err(|error| BackendError::Platform(format!("osascript failed: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(BackendError::Platform(format!(
            "osascript exited with {status}"
        )))
    }
}

fn set_desktop_picture_at(desktop_index: usize, path: &Path) -> Result<(), BackendError> {
    let posix = path
        .to_str()
        .ok_or_else(|| BackendError::Platform("wallpaper path is not valid UTF-8".into()))?;
    let script = format!(
        r#"tell application "System Events"
  try
    set desktopCount to count of desktops
    if {desktop_index} ≤ desktopCount then
      set picture of desktop {desktop_index} to POSIX file "{posix}"
    else
      set picture of desktop 1 to POSIX file "{posix}"
    end if
  end try
end tell"#
    );
    let status = Command::new("osascript")
        .args(["-e", &script])
        .status()
        .map_err(|error| BackendError::Platform(format!("osascript failed: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(BackendError::Platform(format!(
            "osascript exited with {status}"
        )))
    }
}
