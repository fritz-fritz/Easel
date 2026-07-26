// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared still-wallpaper apply path used by automation ticks and Compose.

use std::path::{Path, PathBuf};

use easel_core::{DynamicStillSet, Profile, resolve_displays};
use easel_dynamic::{
    NativeDynamicFormat, cached_bundle_path, encode_per_display_bundles, preferred_native_format,
    prefers_still_frame_host,
};
use easel_library::{
    animated_image_extension, poster_path_for_asset, still_image_extension, video_extension,
};
use easel_platform::{
    DisplayWallpaper, WallpaperOutput, probe_live_wallpaper_backend, select_wallpaper_backend,
};
use easel_render::{
    CompositionSettings, RENDERER_VERSION, RasterJob, RenderPurpose, RenderRequest,
};

use crate::automation_session::automation_store;
use crate::display_session;
use crate::library_session::{library_store, posters_dir};

/// Applies a local still image using the profile composition and hotplug policy.
pub fn apply_still(source: &Path, profile: &Profile) -> Result<String, String> {
    apply_per_display_rasters(source, profile, RenderPurpose::StaticWallpaper, None)
}

/// Applies per-display poster crops for live media when no live host is available.
///
/// `poster_source` must be a still-decodable path (GIF first frame, library video
/// poster PNG, or still image). Live playback is probed for an honest status line;
/// Stage 6.6 always falls back to the still wallpaper backend.
pub fn apply_live_poster_fallback(
    poster_source: &Path,
    profile: &Profile,
) -> Result<String, String> {
    let live = probe_live_wallpaper_backend();
    let note = if live.supported {
        format!("live host {}", live.backend_id.unwrap_or("unknown"))
    } else {
        live.reason
    };
    apply_per_display_rasters(
        poster_source,
        profile,
        RenderPurpose::LivePosterFrame,
        Some(note.as_str()),
    )
}

/// Resolves a still-decodable poster path for a Compose/library motion source.
///
/// Animated images use the container itself (`decode_still` yields the first frame).
/// Video requires a library poster written by the Qt Multimedia probe.
pub fn resolve_live_poster_source(source: &Path) -> Result<PathBuf, String> {
    if !source.is_file() {
        return Err(format!("media file does not exist: {}", source.display()));
    }
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if animated_image_extension(extension) || still_image_extension(extension) {
        return Ok(source.to_path_buf());
    }
    if !video_extension(extension) {
        return Err(format!(
            "unsupported live media extension '.{extension}' for poster fallback"
        ));
    }

    let canonical = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let path_key = canonical.to_string_lossy();
    let library = library_store()?;
    let asset = library
        .find_by_path(path_key.as_ref())
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            "video has no library poster yet — add the folder in Library so Qt can probe a frame"
                .to_owned()
        })?;
    let poster = poster_path_for_asset(&posters_dir(), asset.id);
    if !poster.is_file() {
        return Err(
            "video library entry exists but poster PNG is missing — re-index the folder".into(),
        );
    }
    Ok(poster)
}

fn apply_per_display_rasters(
    source: &Path,
    profile: &Profile,
    purpose: RenderPurpose,
    live_note: Option<&str>,
) -> Result<String, String> {
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
        purpose,
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

    let mut message = format!("applied via {} ({})", backend.id(), resolution.reason);
    if purpose == RenderPurpose::LivePosterFrame {
        message = format!("poster fallback {message}");
    }
    if let Some(note) = live_note {
        message = format!("{message}; {note}");
    }
    Ok(message)
}

/// Outcome of preferring a native dynamic HEIC host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeDynamicApply {
    /// Packages for this arrangement are already hosted by the OS.
    AlreadyHosting {
        /// Arrangement fingerprint stored in dynamic-still state.
        fingerprint: String,
    },
    /// Encoded and applied native packages.
    Applied {
        /// Human-readable status.
        message: String,
        /// Arrangement fingerprint to persist.
        fingerprint: String,
    },
}

/// Encodes (or reuses) per-display native dynamic HEIC packages and hands them to the OS.
pub fn apply_native_dynamic(
    set: &DynamicStillSet,
    frame_paths: &[PathBuf],
    profile: &Profile,
    last_host_fingerprint: Option<&str>,
) -> Result<NativeDynamicApply, String> {
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
    let composition = CompositionSettings::from_profile(&request_profile);
    let output_dir = native_bundle_cache_dir(set);
    let fingerprint = native_host_fingerprint(set, &displays, &composition);
    let stored = last_host_fingerprint
        .map(str::to_owned)
        .or_else(|| read_native_host_fingerprint(set));
    let host_hint = native_format_for_backend();
    let format = preferred_native_format(set, host_hint);
    // Dense solar/h24 on Plasma: evaluate in Rust and publish still frames (Easel
    // plugin IPC when installed). Do not require zzag or other external HEIC hosts.
    if prefers_still_frame_host(set, host_hint) {
        return Err(
            "dense solar/h24 uses Easel still-frame host; falling back to still poller".into(),
        );
    }
    if stored.as_deref() == Some(fingerprint.as_str())
        && displays
            .iter()
            .all(|display| cached_bundle_path(set, display.id, &output_dir, format).is_some())
    {
        return Ok(NativeDynamicApply::AlreadyHosting { fingerprint });
    }

    let encoded = encode_per_display_bundles(
        set,
        frame_paths,
        &displays,
        &composition,
        format,
        &output_dir,
    )
    .map_err(|error| error.to_string())?;

    let mut wallpapers = Vec::with_capacity(encoded.len());
    for bundle in encoded {
        let logical_rect = displays
            .iter()
            .find(|display| display.id == bundle.display_id)
            .map(|display| display.logical_rect)
            .ok_or_else(|| "display missing for native bundle".to_owned())?;
        wallpapers.push(DisplayWallpaper {
            display_id: bundle.display_id,
            path: bundle.path,
            logical_rect,
        });
    }

    let backend = select_wallpaper_backend().map_err(|error| error.to_string())?;
    if !backend.capabilities().native_dynamic_bundle {
        return Err("backend cannot host native dynamic bundles".into());
    }
    backend
        .apply(&WallpaperOutput::NativeDynamic(wallpapers))
        .map_err(|error| error.to_string())?;
    write_native_host_fingerprint(set, &fingerprint)?;

    Ok(NativeDynamicApply::Applied {
        message: format!(
            "native dynamic via {} ({})",
            backend.id(),
            resolution.reason
        ),
        fingerprint,
    })
}

/// Fingerprint of still-set + arrangement used to skip redundant native re-apply.
#[must_use]
pub fn native_host_fingerprint(
    set: &DynamicStillSet,
    displays: &[easel_core::Display],
    composition: &CompositionSettings,
) -> String {
    use std::fmt::Write as _;
    let mut material = format!(
        "v{RENDERER_VERSION}|{}|{:?}|{:?}|z{:.4}|fx{:.4}|fy{:.4}|pkg={}",
        set.id.to_hyphenated_string(),
        composition.layout_mode,
        composition.fit_mode,
        composition.zoom,
        composition.focal_x,
        composition.focal_y,
        set.source_package_path.as_deref().unwrap_or("")
    );
    for frame in &set.frames {
        let _ = write!(
            material,
            "|f{}:{}",
            frame
                .source_index
                .map_or_else(|| "-".into(), |index| index.to_string()),
            frame.asset_id.to_hyphenated_string()
        );
    }
    for display in displays {
        let _ = write!(
            material,
            "|{}:{}x{}@{},{}",
            display.id.to_hyphenated_string(),
            display.native_pixels.width,
            display.native_pixels.height,
            display.logical_rect.x,
            display.logical_rect.y
        );
    }
    material
}

/// Returns the automation apply cache directory (tests / diagnostics).
#[must_use]
pub fn apply_cache_dir() -> PathBuf {
    std::env::temp_dir().join("easel").join("automation-apply")
}

/// Cache directory for encoded per-display native dynamic packages.
#[must_use]
pub fn native_bundle_cache_dir(set: &DynamicStillSet) -> PathBuf {
    apply_cache_dir()
        .join("native-dynamic")
        .join(set.id.to_hyphenated_string())
}

fn fingerprint_path(set: &DynamicStillSet) -> PathBuf {
    native_bundle_cache_dir(set).join("host.fingerprint")
}

fn read_native_host_fingerprint(set: &DynamicStillSet) -> Option<String> {
    std::fs::read_to_string(fingerprint_path(set))
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn write_native_host_fingerprint(set: &DynamicStillSet, fingerprint: &str) -> Result<(), String> {
    let path = fingerprint_path(set);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, fingerprint).map_err(|error| error.to_string())
}

fn native_format_for_backend() -> NativeDynamicFormat {
    match easel_platform::select_wallpaper_backend() {
        Ok(backend) if backend.id() == "plasma6" => NativeDynamicFormat::PlasmaDayNight,
        Ok(backend) if backend.id() == "macos" => NativeDynamicFormat::AppleHeic,
        _ => NativeDynamicFormat::AppleHeic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_cache_dir_is_under_temp() {
        let path = apply_cache_dir();
        assert!(path.ends_with("automation-apply"));
    }

    #[test]
    fn plasma_dense_solar_skips_native_host() {
        use easel_core::{AssetId, DynamicScheduleKind, DynamicStillSet, ProfileId};

        let asset = AssetId::new();
        let mut set = DynamicStillSet::default_hourly("Solar", ProfileId::new(), asset).unwrap();
        set.schedule_kind = DynamicScheduleKind::SolarPosition;
        assert!(prefers_still_frame_host(
            &set,
            NativeDynamicFormat::PlasmaDayNight
        ));
    }

    #[test]
    fn resolve_live_poster_source_uses_still_path() {
        let path =
            std::env::temp_dir().join(format!("easel-live-poster-{}.png", std::process::id()));
        // Minimal valid 1×1 PNG.
        let png = [
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00,
            0x00, 0x90, 0x77, 0x53, 0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x08,
            0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x05, 0xfe,
            0xd4, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        std::fs::write(&path, png).unwrap();
        let resolved = resolve_live_poster_source(&path).expect("still poster source");
        assert_eq!(resolved, path);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn resolve_live_poster_source_rejects_unindexed_video() {
        let path =
            std::env::temp_dir().join(format!("easel-live-poster-{}.mp4", std::process::id()));
        std::fs::write(&path, b"not a real video").unwrap();
        let err = resolve_live_poster_source(&path).expect_err("video needs poster");
        assert!(err.contains("library poster") || err.contains("Library"));
        let _ = std::fs::remove_file(path);
    }
}
