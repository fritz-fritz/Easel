// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared still-wallpaper apply path used by Compose automation and CLI.

use std::path::{Path, PathBuf};

use easel_core::{Display, DisplayGroup, Profile, filter_displays, resolve_hotplug};
use easel_platform::{DisplayWallpaper, WallpaperOutput, select_wallpaper_backend};
use easel_render::{CompositionSettings, RasterJob, RenderPurpose, RenderRequest};
use easel_scheduler::TickDecision;

use crate::automation_session;
use crate::display_session;

/// Applies a local still image using the active profile composition and hotplug policy.
pub fn apply_still(source: &Path, profile: &Profile) -> Result<String, String> {
    let live = display_session::current_displays();
    if live.is_empty() {
        return Err("no displays available".into());
    }

    let catalog = automation_session::lock()?;
    let group = resolve_group(profile, &catalog, &live)?;
    let resolution = resolve_hotplug(&group, &live, catalog.missing_output_policy);
    if !resolution.may_apply {
        return Err(resolution.reason);
    }
    let displays = filter_displays(&live, &resolution.present);
    drop(catalog);

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

fn resolve_group(
    profile: &Profile,
    catalog: &easel_scheduler::AutomationCatalog,
    live: &[Display],
) -> Result<DisplayGroup, String> {
    if let Some(group_id) = profile.display_group_id {
        if let Some(group) = catalog.display_group(group_id) {
            return Ok(group.clone());
        }
    }
    if !profile.displays.is_empty() {
        let group = DisplayGroup::new(
            format!("{} displays", profile.name),
            profile.displays.clone(),
        );
        group.validate().map_err(|error| error.to_string())?;
        return Ok(group);
    }
    let displays: Vec<_> = live.iter().map(|display| display.id).collect();
    let group = DisplayGroup::new("All displays", displays);
    group.validate().map_err(|error| error.to_string())?;
    Ok(group)
}

/// Executes a scheduler tick and, when due, applies the selected asset.
pub fn run_automation_tick(force: bool) -> Result<String, String> {
    let decision = automation_session::run_tick(force)?;
    match decision {
        TickDecision::Paused { reason }
        | TickDecision::Idle { reason, .. }
        | TickDecision::Failed { reason } => Ok(reason),
        TickDecision::Apply {
            selection,
            schedule,
        } => {
            let path = automation_session::resolve_asset_path(selection.asset_id)?;
            let catalog = automation_session::lock()?;
            let profile = catalog
                .state
                .active_profile_id
                .and_then(|id| catalog.profile(id))
                .cloned()
                .unwrap_or_else(|| Profile::new("Automation"));
            drop(catalog);

            let apply_message = apply_still(&path, &profile)?;
            let decision_text = format!(
                "{}; {}; {}",
                selection.reason, schedule.reason, apply_message
            );
            automation_session::record_apply(selection.asset_id, &decision_text)?;
            Ok(decision_text)
        }
    }
}

/// Builds a profile snapshot from Compose controls for persistence.
#[must_use]
pub fn profile_from_compose(
    name: &str,
    source: Option<&Path>,
    fit_mode_index: i32,
    layout_mode_index: i32,
    zoom: f64,
    focal_x: f64,
    focal_y: f64,
) -> Profile {
    let displays = display_session::current_displays();
    let mut profile = Profile::new(name);
    profile.fit_mode = match fit_mode_index {
        1 => easel_core::FitMode::Contain,
        2 => easel_core::FitMode::Stretch,
        3 => easel_core::FitMode::Native,
        _ => easel_core::FitMode::Cover,
    };
    profile.layout_mode = match layout_mode_index {
        1 => easel_core::LayoutMode::Digital,
        _ => easel_core::LayoutMode::PhysicalSpan,
    };
    profile.zoom = if zoom.is_finite() { zoom.max(1.0) } else { 1.0 };
    profile.focal_x = focal_x.clamp(0.0, 1.0);
    profile.focal_y = focal_y.clamp(0.0, 1.0);
    profile.displays = displays.iter().map(|display| display.id).collect();
    if let Some(path) = source {
        if let Some(asset) = automation_session::find_asset_by_path(path) {
            profile.selected_asset = Some(asset.id);
        }
    }
    if let Ok(group) = automation_session::ensure_default_group(&displays) {
        profile.display_group_id = Some(group.id);
    }
    profile
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
