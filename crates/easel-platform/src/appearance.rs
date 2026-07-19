// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Probe the host light/dark appearance preference.

use std::process::Command;

use easel_core::AppearanceMode;

/// Best-effort probe of the current system light/dark preference.
///
/// Returns [`AppearanceMode::Light`] when the preference cannot be determined.
#[must_use]
pub fn system_appearance() -> AppearanceMode {
    #[cfg(target_os = "macos")]
    {
        return macos_appearance();
    }
    #[cfg(windows)]
    {
        return windows_appearance();
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        linux_appearance()
    }
}

#[cfg(target_os = "macos")]
fn macos_appearance() -> AppearanceMode {
    let Ok(output) = Command::new("defaults")
        .args(["read", "-g", "AppleInterfaceStyle"])
        .output()
    else {
        return AppearanceMode::Light;
    };
    let value = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    if value.contains("dark") {
        AppearanceMode::Dark
    } else {
        AppearanceMode::Light
    }
}

#[cfg(windows)]
fn windows_appearance() -> AppearanceMode {
    // AppsUseLightTheme = 0 → dark mode.
    let Ok(output) = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ])
        .output()
    else {
        return AppearanceMode::Light;
    };
    let value = String::from_utf8_lossy(&output.stdout);
    if value.contains("0x0") {
        AppearanceMode::Dark
    } else {
        AppearanceMode::Light
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn linux_appearance() -> AppearanceMode {
    if let Some(mode) = plasma_color_scheme() {
        return mode;
    }
    if let Some(mode) = gnome_color_scheme() {
        return mode;
    }
    AppearanceMode::Light
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn plasma_color_scheme() -> Option<AppearanceMode> {
    let output = Command::new("kreadconfig6")
        .args([
            "--file",
            "kdeglobals",
            "--group",
            "General",
            "--key",
            "ColorScheme",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    if value.contains("dark") || value.contains("breeze dark") {
        Some(AppearanceMode::Dark)
    } else if value.trim().is_empty() {
        None
    } else {
        Some(AppearanceMode::Light)
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn gnome_color_scheme() -> Option<AppearanceMode> {
    let output = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    if value.contains("prefer-dark") {
        Some(AppearanceMode::Dark)
    } else {
        Some(AppearanceMode::Light)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_appearance_returns_light_or_dark() {
        match system_appearance() {
            AppearanceMode::Light | AppearanceMode::Dark => {}
        }
    }
}
