// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared still-wallpaper apply path used by automation ticks and Compose.

use std::path::{Path, PathBuf};

use easel_core::{Profile, resolve_displays};
use easel_platform::{DisplayWallpaper, WallpaperOutput, select_wallpaper_backend};
use easel_render::{CompositionSettings, RasterJob, RenderPurpose, RenderRequest};

use crate::automation_session::automation_store;
use crate::display_session;

/// Applies a local still image using the profile composition and hotplug policy.
pub fn apply_still(source: &Path, profile: &Profile) -> Result<String, String> {
    let live = display_session::current_displays();
    if live.is_empty() {
        return Err("no displays available".into());
    }

    let policy = automation_store()?.hotplug_policy().clone();
    let resolution = resolve_displays(profile, &live, &policy);
    if !resolution.should_apply {
        return Err(resolution.reason);
    }
    let displays = resolution.active_displays;
    if displays.is_empty() {
        return Err("hotplug resolution produced no displays".into());
    }

    let mut request_profile = profile.clone();
    request_profile.displays = displays.iter().map(|display| display.id).collect();

    let request = RenderRequest {
        source_path: source.to_path_buf(),
        displays: displays.clone(),
        composition: CompositionSettings::from_profile(&request_profile),
        purpose: RenderPurpose::StaticWallpaper,
    };
    let output_dir = apply_cache_dir();
    let outputs = RasterJob {
        request,
        output_dir,
    }
    .execute()
    .map_err(|error| error.to_string())?;

    let mut wallpapers = Vec::with_capacity(outputs.len());
    for output in outputs {
        let logical_rect = displays
            .iter()
            .find(|display| display.id == output.display_id)
            .map(|display| display.logical_rect)
            .ok_or_else(|| "display missing for raster output".to_owned())?;
        wallpapers.push(DisplayWallpaper {
            display_id: output.display_id,
            path: output.path,
            logical_rect,
        });
    }

    let backend = select_wallpaper_backend().map_err(|error| error.to_string())?;
    backend
        .apply(&WallpaperOutput::PerDisplay(wallpapers))
        .map_err(|error| error.to_string())?;

    Ok(format!(
        "applied via {} ({})",
        backend.id(),
        resolution.reason
    ))
}

/// Returns the automation apply cache directory (tests / diagnostics).
#[must_use]
pub fn apply_cache_dir() -> PathBuf {
    std::env::temp_dir().join("easel").join("automation-apply")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_cache_dir_is_under_temp() {
        let path = apply_cache_dir();
        assert!(path.ends_with("automation-apply"));
    }
}
