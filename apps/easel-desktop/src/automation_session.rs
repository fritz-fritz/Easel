// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Persisted automation catalog (profiles, groups, queues, schedules, state).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use directories::ProjectDirs;
use easel_core::{
    AssetId, AssetLocation, Display, DisplayGroup, DisplayId, MediaAsset, MissingOutputPolicy,
    Profile, RotationQueue, Schedule,
};
use easel_scheduler::{AutomationCatalog, TickDecision, tick};

use crate::display_session;
use crate::library_session;

static CATALOG: OnceLock<Mutex<AutomationCatalog>> = OnceLock::new();

fn catalog_mutex() -> &'static Mutex<AutomationCatalog> {
    CATALOG.get_or_init(|| Mutex::new(load_or_default()))
}

/// Locks the process-wide automation catalog.
pub fn lock() -> Result<MutexGuard<'static, AutomationCatalog>, String> {
    catalog_mutex()
        .lock()
        .map_err(|_| "automation catalog lock poisoned".into())
}

/// Reloads the catalog from disk into the process session.
#[allow(dead_code)]
pub fn reload() -> Result<(), String> {
    let loaded = load_or_default();
    let mut guard = lock()?;
    *guard = loaded;
    Ok(())
}

/// Persists the current catalog atomically.
pub fn save(catalog: &AutomationCatalog) -> Result<(), String> {
    catalog.validate().map_err(|error| error.to_string())?;
    fs::create_dir_all(config_dir()).map_err(|error| error.to_string())?;
    let text = toml::to_string_pretty(catalog).map_err(|error| error.to_string())?;
    atomic_write(&catalog_path(), text.as_bytes())
}

fn load_or_default() -> AutomationCatalog {
    load_catalog(&catalog_path()).unwrap_or_else(|_| AutomationCatalog::default())
}

fn load_catalog(path: &Path) -> Result<AutomationCatalog, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let catalog: AutomationCatalog = toml::from_str(&text).map_err(|error| error.to_string())?;
    catalog.validate().map_err(|error| error.to_string())?;
    Ok(catalog)
}

fn config_dir() -> PathBuf {
    ProjectDirs::from("net", "fritztech", "easel").map_or_else(
        || PathBuf::from(".").join("easel-config"),
        |dirs| dirs.config_dir().to_path_buf(),
    )
}

fn catalog_path() -> PathBuf {
    config_dir().join("automation.toml")
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("toml.part");
    let stash = path.with_extension("toml.bak");
    {
        let mut file = fs::File::create(&temp).map_err(|error| error.to_string())?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
    }
    let had_existing = path.exists();
    if had_existing {
        let _ = fs::remove_file(&stash);
        fs::rename(path, &stash).map_err(|error| {
            let _ = fs::remove_file(&temp);
            error.to_string()
        })?;
    }
    match fs::rename(&temp, path) {
        Ok(()) => {
            let _ = fs::remove_file(&stash);
            Ok(())
        }
        Err(error) => {
            if had_existing {
                let _ = fs::rename(&stash, path);
            }
            let _ = fs::remove_file(&temp);
            Err(error.to_string())
        }
    }
}

/// Ensures a default "All displays" group exists for the live arrangement.
pub fn ensure_default_group(live: &[Display]) -> Result<DisplayGroup, String> {
    let mut catalog = lock()?;
    if let Some(group) = catalog.display_groups.first().cloned() {
        return Ok(group);
    }
    let displays: Vec<DisplayId> = live.iter().map(|display| display.id).collect();
    if displays.is_empty() {
        return Err("no displays available for a default group".into());
    }
    let group = DisplayGroup::new("All displays", displays);
    group.validate().map_err(|error| error.to_string())?;
    catalog.upsert_display_group(group.clone());
    save(&catalog)?;
    Ok(group)
}

/// Saves a profile and links it as the active automation profile.
pub fn save_profile(profile: Profile) -> Result<(), String> {
    profile.validate().map_err(|error| error.to_string())?;
    let mut catalog = lock()?;
    catalog.state.active_profile_id = Some(profile.id);
    if let Some(schedule_id) = profile.schedule_id {
        catalog.state.active_schedule_id = Some(schedule_id);
    }
    if let Some(queue_id) = profile.rotation_queue_id {
        catalog.state.active_queue_id = Some(queue_id);
    }
    catalog.upsert_profile(profile);
    save(&catalog)
}

/// Creates or updates an interval schedule and activates it.
pub fn set_interval_schedule(name: &str, seconds: u64) -> Result<Schedule, String> {
    let schedule = Schedule::interval(name, seconds);
    schedule.validate().map_err(|error| error.to_string())?;
    let mut catalog = lock()?;
    catalog.state.active_schedule_id = Some(schedule.id);
    catalog.upsert_schedule(schedule.clone());
    save(&catalog)?;
    Ok(schedule)
}

/// Creates or replaces a rotation queue from asset ids and activates it.
pub fn set_rotation_queue(name: &str, assets: Vec<AssetId>) -> Result<RotationQueue, String> {
    let mut queue = RotationQueue::new(name);
    queue.assets = assets;
    queue.validate().map_err(|error| error.to_string())?;
    let mut catalog = lock()?;
    catalog.state.active_queue_id = Some(queue.id);
    catalog.upsert_rotation_queue(queue.clone());
    save(&catalog)?;
    Ok(queue)
}

/// Sets pause state and persists it.
pub fn set_paused(paused: bool) -> Result<(), String> {
    let mut catalog = lock()?;
    catalog.state.paused = paused;
    catalog.state.last_decision = if paused {
        "paused by user".into()
    } else {
        "resumed by user".into()
    };
    save(&catalog)
}

/// Updates the missing-output hotplug policy.
pub fn set_missing_output_policy(policy: MissingOutputPolicy) -> Result<(), String> {
    let mut catalog = lock()?;
    catalog.missing_output_policy = policy;
    save(&catalog)
}

/// Resolves a local filesystem path for an asset id via the library store.
pub fn resolve_asset_path(asset_id: AssetId) -> Result<PathBuf, String> {
    let store = library_session::library_store()?;
    let asset = store
        .get_asset(asset_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| {
            format!(
                "asset {} not found in library",
                asset_id.to_hyphenated_string()
            )
        })?;
    match asset.location {
        AssetLocation::Local { path } => Ok(PathBuf::from(path)),
        AssetLocation::Remote { .. } => Err(
            "remote assets must be acquired into the library before automation can apply them"
                .into(),
        ),
    }
}

/// Looks up an asset by local path in the library, if present.
pub fn find_asset_by_path(path: &Path) -> Option<MediaAsset> {
    let Ok(store) = library_session::library_store() else {
        return None;
    };
    store.find_by_path(&path.to_string_lossy()).ok().flatten()
}

/// Runs one scheduler tick; when `force`, acts like skip/apply-next.
pub fn run_tick(force: bool) -> Result<TickDecision, String> {
    let catalog = lock()?;
    let now = unix_now();
    let schedule = catalog
        .state
        .active_schedule_id
        .and_then(|id| catalog.schedule(id))
        .cloned();
    let queue = catalog
        .state
        .active_queue_id
        .and_then(|id| catalog.rotation_queue(id))
        .cloned();
    Ok(tick(
        &catalog.state,
        schedule.as_ref(),
        queue.as_ref(),
        now,
        force,
    ))
}

/// Records a successful automated apply into catalog state.
pub fn record_apply(asset_id: AssetId, decision: &str) -> Result<(), String> {
    let mut catalog = lock()?;
    catalog.state.record_apply(asset_id, unix_now(), decision);
    save(&catalog)
}

/// Summarizes automation status for CLI and UI.
pub fn status_summary() -> Result<String, String> {
    let catalog = lock()?;
    let profile = catalog
        .state
        .active_profile_id
        .and_then(|id| catalog.profile(id))
        .map_or("(none)", |profile| profile.name.as_str());
    let schedule = catalog
        .state
        .active_schedule_id
        .and_then(|id| catalog.schedule(id))
        .map_or("(none)", |schedule| schedule.name.as_str());
    let queue = catalog
        .state
        .active_queue_id
        .and_then(|id| catalog.rotation_queue(id))
        .map_or_else(
            || "(none)".into(),
            |queue| format!("{} ({} assets)", queue.name, queue.assets.len()),
        );
    let live = display_session::current_displays();
    let group = catalog
        .state
        .active_profile_id
        .and_then(|id| catalog.profile(id))
        .and_then(|profile| profile.display_group_id)
        .and_then(|id| catalog.display_group(id));
    let hotplug = if let Some(group) = group {
        let resolution = easel_core::resolve_hotplug(group, &live, catalog.missing_output_policy);
        format!(
            "hotplug: may_apply={} ({})",
            resolution.may_apply, resolution.reason
        )
    } else {
        format!("hotplug policy: {:?}", catalog.missing_output_policy)
    };
    Ok(format!(
        "paused={}\nprofile={profile}\nschedule={schedule}\nqueue={queue}\nlast_applied={:?}\nlast_decision={}\n{hotplug}\nprofiles={} groups={} queues={} schedules={}",
        catalog.state.paused,
        catalog.state.last_applied_unix,
        catalog.state.last_decision,
        catalog.profiles.len(),
        catalog.display_groups.len(),
        catalog.rotation_queues.len(),
        catalog.schedules.len(),
    ))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}
