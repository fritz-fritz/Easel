// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Automation-page presentation model, pause/skip controls, and schedule polling.

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QString, QStringList};
use easel_core::{
    AssetId, AssetLocation, HotplugPolicy, InstantSeconds, MissingOutputPolicy, Profile, ProfileId,
    RotationSource, resolve_displays,
};
use easel_scheduler::now_unix_i64;
use serde_json::json;

use crate::apply_service;
use crate::automation_session::automation_store;
use crate::display_session::current_displays;
use crate::library_session::library_store;

#[cxx_qt::bridge]
mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, status_text)]
        #[qproperty(QStringList, schedule_model)]
        #[qproperty(bool, paused)]
        #[qproperty(QString, next_fire_hint)]
        #[qproperty(QString, last_apply_reason)]
        #[qproperty(i32, hotplug_policy_index)]
        type AutomationController = super::AutomationControllerRust;

        #[qinvokable]
        #[rust_name = "set_utc_offset_minutes"]
        fn setUtcOffsetMinutes(self: Pin<&mut Self>, minutes: i32);

        #[qinvokable]
        #[rust_name = "refresh"]
        fn refresh(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "set_rotation_paused"]
        fn setRotationPaused(self: Pin<&mut Self>, paused: bool);

        #[qinvokable]
        #[rust_name = "skip_next"]
        fn skipNext(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "apply_hotplug_policy_index"]
        fn applyHotplugPolicyIndex(self: Pin<&mut Self>, index: i32);

        #[qinvokable]
        #[rust_name = "poll_due_schedules"]
        fn pollDueSchedules(self: Pin<&mut Self>);

        #[qinvokable]
        #[rust_name = "evaluate_hotplug"]
        fn evaluateHotplug(self: Pin<&mut Self>);
    }
}

/// Presentation state for the Automation page and tray actions.
pub struct AutomationControllerRust {
    status_text: QString,
    schedule_model: QStringList,
    paused: bool,
    next_fire_hint: QString,
    last_apply_reason: QString,
    hotplug_policy_index: i32,
    /// Local offset east of UTC in minutes (from QML `Date.getTimezoneOffset()` negated).
    utc_offset_minutes: i32,
}

impl Default for AutomationControllerRust {
    fn default() -> Self {
        let mut controller = Self {
            status_text: QString::from("Configure schedules from Compose Save profile."),
            schedule_model: QStringList::default(),
            paused: false,
            next_fire_hint: QString::from("none"),
            last_apply_reason: QString::from("none"),
            hotplug_policy_index: 0,
            utc_offset_minutes: 0,
        };
        let _ = controller.reload_models();
        controller
    }
}

impl AutomationControllerRust {
    fn reload_models(&mut self) -> Result<(), String> {
        let store = automation_store()?;
        let summary = store
            .summary(self.utc_offset_minutes)
            .map_err(|error| error.to_string())?;
        self.paused = summary.any_paused;
        self.next_fire_hint = QString::from(summary.next_fire_hint.as_deref().unwrap_or("none"));
        self.last_apply_reason =
            QString::from(summary.last_apply_reason.as_deref().unwrap_or("none"));
        self.hotplug_policy_index = match store.hotplug_policy().on_missing {
            MissingOutputPolicy::SkipMissing => 0,
            MissingOutputPolicy::DeferUntilComplete => 1,
            MissingOutputPolicy::UseAllConnected => 2,
        };
        self.schedule_model = {
            let mut list = QStringList::default();
            for schedule in store.schedules() {
                let row = json!({
                    "id": schedule.id.to_hyphenated_string(),
                    "name": schedule.name,
                    "enabled": schedule.enabled,
                    "profileId": schedule.profile_id.to_hyphenated_string(),
                    "rule": format!("{:?}", schedule.rule),
                })
                .to_string();
                list.append(QString::from(row.as_str()));
            }
            list
        };
        self.status_text = QString::from(
            format!(
                "{} schedule(s), {} still set(s), paused={}, hotplug={:?}",
                store.schedules().len(),
                store.still_sets().len(),
                self.paused,
                store.hotplug_policy().on_missing
            )
            .as_str(),
        );
        if let Some(hint) = summary.next_dynamic_hint.as_deref() {
            self.next_fire_hint = QString::from(
                format!(
                    "{}; dynamic {}",
                    summary.next_fire_hint.as_deref().unwrap_or("none"),
                    hint
                )
                .as_str(),
            );
        }
        Ok(())
    }
}

impl qobject::AutomationController {
    fn set_utc_offset_minutes(mut self: Pin<&mut Self>, minutes: i32) {
        self.as_mut().rust_mut().utc_offset_minutes = minutes.clamp(-14 * 60, 14 * 60);
        match self.as_mut().rust_mut().reload_models() {
            Ok(()) => publish_state(self),
            Err(error) => {
                self.as_mut().set_status_text(QString::from(error.as_str()));
            }
        }
    }

    fn refresh(mut self: Pin<&mut Self>) {
        match self.as_mut().rust_mut().reload_models() {
            Ok(()) => publish_state(self),
            Err(error) => {
                self.as_mut().set_status_text(QString::from(error.as_str()));
            }
        }
    }

    fn set_rotation_paused(mut self: Pin<&mut Self>, paused: bool) {
        match automation_store().and_then(|mut store| {
            store
                .set_all_paused(paused)
                .map_err(|error| error.to_string())
        }) {
            Ok(()) => self.refresh(),
            Err(error) => {
                self.as_mut().set_status_text(QString::from(error.as_str()));
            }
        }
    }

    fn skip_next(mut self: Pin<&mut Self>) {
        let result = (|| {
            use std::path::Path;

            use easel_scheduler::{RotationHistoryEntry, now_unix_i64};

            let profile_id = {
                let store = automation_store()?;
                store
                    .profiles()
                    .iter()
                    .find(|profile| profile.rotation_queue_id.is_some())
                    .map(|profile| profile.id)
                    .ok_or_else(|| "no profile with a rotation queue".to_string())?
            };
            let membership = membership_for(profile_id)?;
            {
                let mut store = automation_store()?;
                let _ = store
                    .skip_for_profile(profile_id, &membership)
                    .map_err(|error| error.to_string())?;
            }
            let (queue_id, decision, profile) = {
                let store = automation_store()?;
                let (queue_id, decision) = store
                    .select_for_profile(profile_id, &membership)
                    .map_err(|error| error.to_string())?;
                let profile = store
                    .profile(profile_id)
                    .cloned()
                    .ok_or_else(|| "profile not found".to_string())?;
                (queue_id, decision, profile)
            };
            let path = resolve_asset_path(decision.asset_id)?;
            let apply_message = apply_service::apply_still(Path::new(&path), &profile)?;
            let mut store = automation_store()?;
            store
                .commit_selection(queue_id, decision.next_cursor)
                .map_err(|error| error.to_string())?;
            let occurred_at = now_unix_i64();
            let reason = format!("skip; {}; {}", decision.reason, apply_message);
            store
                .history()
                .record(&RotationHistoryEntry {
                    queue_id: Some(queue_id),
                    profile_id,
                    schedule_id: profile.schedule_id,
                    asset_id: decision.asset_id,
                    reason: reason.clone(),
                    occurred_at,
                })
                .map_err(|error| error.to_string())?;
            Ok::<_, String>(reason)
        })();
        match result {
            Ok(reason) => {
                self.as_mut()
                    .set_status_text(QString::from(reason.as_str()));
                self.refresh();
            }
            Err(error) => {
                self.as_mut().set_status_text(QString::from(error.as_str()));
            }
        }
    }

    fn apply_hotplug_policy_index(mut self: Pin<&mut Self>, index: i32) {
        let on_missing = match index {
            1 => MissingOutputPolicy::DeferUntilComplete,
            2 => MissingOutputPolicy::UseAllConnected,
            _ => MissingOutputPolicy::SkipMissing,
        };
        let policy = HotplugPolicy {
            on_missing,
            ..HotplugPolicy::default()
        };
        match automation_store().and_then(|mut store| {
            store
                .set_hotplug_policy(policy)
                .map_err(|error| error.to_string())
        }) {
            Ok(()) => self.refresh(),
            Err(error) => {
                self.as_mut().set_status_text(QString::from(error.as_str()));
            }
        }
    }

    fn poll_due_schedules(mut self: Pin<&mut Self>) {
        let utc_offset_minutes = self.as_ref().rust().utc_offset_minutes;
        let result: Result<String, String> = (|| {
            let now = InstantSeconds {
                unix_seconds: now_unix_i64(),
            };
            let due = {
                let store = automation_store()?;
                store
                    .due_schedules(now, utc_offset_minutes)
                    .map_err(|error| error.to_string())?
            };
            let mut messages = Vec::new();
            for schedule in due {
                match note_due_schedule(schedule.profile_id, schedule.id, &schedule.name) {
                    Ok(message) => messages.push(message),
                    Err(error) => messages.push(error),
                }
            }
            match sync_dynamic_stills(now, utc_offset_minutes) {
                Ok(dynamic_messages) => messages.extend(dynamic_messages),
                Err(error) => messages.push(error),
            }
            if messages.is_empty() {
                return Ok("No due schedules or dynamic transitions.".to_string());
            }
            Ok(messages.join(" | "))
        })();
        match result {
            Ok(message) => {
                self.as_mut()
                    .set_status_text(QString::from(message.as_str()));
                self.refresh();
            }
            Err(error) => {
                self.as_mut().set_status_text(QString::from(error.as_str()));
            }
        }
    }

    fn evaluate_hotplug(mut self: Pin<&mut Self>) {
        let result = (|| {
            let store = automation_store()?;
            let policy = store.hotplug_policy().clone();
            let connected = current_displays();
            let mut messages = Vec::new();
            for profile in store.profiles() {
                let resolution = resolve_displays(profile, &connected, &policy);
                messages.push(format!("{}: {}", profile.name, resolution.reason));
            }
            if messages.is_empty() {
                messages.push("No profiles to evaluate.".into());
            }
            Ok::<_, String>(messages.join(" | "))
        })();
        match result {
            Ok(message) => {
                self.as_mut()
                    .set_status_text(QString::from(message.as_str()));
            }
            Err(error) => {
                self.as_mut().set_status_text(QString::from(error.as_str()));
            }
        }
    }
}

fn publish_state(mut controller: Pin<&mut qobject::AutomationController>) {
    let status = controller.as_ref().rust().status_text.clone();
    let schedules = controller.as_ref().rust().schedule_model.clone();
    let paused = controller.as_ref().rust().paused;
    let next = controller.as_ref().rust().next_fire_hint.clone();
    let last = controller.as_ref().rust().last_apply_reason.clone();
    let hotplug = controller.as_ref().rust().hotplug_policy_index;
    controller.as_mut().set_status_text(status);
    controller.as_mut().set_schedule_model(schedules);
    controller.as_mut().set_paused(paused);
    controller.as_mut().set_next_fire_hint(next);
    controller.as_mut().set_last_apply_reason(last);
    controller.as_mut().set_hotplug_policy_index(hotplug);
}

fn membership_for(profile_id: ProfileId) -> Result<Vec<AssetId>, String> {
    let store = automation_store()?;
    let profile = store
        .profile(profile_id)
        .ok_or_else(|| "profile not found".to_string())?
        .clone();
    let queue_id = profile
        .rotation_queue_id
        .ok_or_else(|| "profile has no rotation queue".to_string())?;
    let queue = store
        .queue(queue_id)
        .ok_or_else(|| "queue not found".to_string())?
        .clone();
    match queue.source {
        RotationSource::Assets { asset_ids } => Ok(asset_ids),
        RotationSource::Collection { collection_id } => {
            let library = library_store()?;
            let collection = library
                .get_collection(collection_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "collection not found".to_string())?;
            Ok(collection.asset_ids)
        }
    }
}

fn note_due_schedule(
    profile_id: ProfileId,
    schedule_id: easel_core::ScheduleId,
    schedule_name: &str,
) -> Result<String, String> {
    use std::path::Path;

    use easel_scheduler::{RotationHistoryEntry, now_unix_i64};

    let membership = membership_for(profile_id)?;
    let (queue_id, decision, profile) = {
        let store = automation_store()?;
        let (queue_id, decision) = store
            .select_for_profile(profile_id, &membership)
            .map_err(|error| error.to_string())?;
        let profile = store
            .profile(profile_id)
            .cloned()
            .ok_or_else(|| "profile not found".to_string())?;
        (queue_id, decision, profile)
    };
    let path = resolve_asset_path(decision.asset_id)?;
    let apply_message = apply_service::apply_still(Path::new(&path), &profile)?;

    let mut store = automation_store()?;
    store
        .commit_selection(queue_id, decision.next_cursor)
        .map_err(|error| error.to_string())?;
    let occurred_at = now_unix_i64();
    let reason = format!("{}; {}", decision.reason, apply_message);
    store
        .history()
        .record(&RotationHistoryEntry {
            queue_id: Some(queue_id),
            profile_id,
            schedule_id: Some(schedule_id),
            asset_id: decision.asset_id,
            reason: reason.clone(),
            occurred_at,
        })
        .map_err(|error| error.to_string())?;
    store
        .history()
        .set_last_fired(schedule_id, occurred_at)
        .map_err(|error| error.to_string())?;

    Ok(format!(
        "schedule '{schedule_name}' applied {} ({path})",
        decision.asset_id.to_hyphenated_string()
    ))
}

fn sync_dynamic_stills(
    now: InstantSeconds,
    utc_offset_minutes: i32,
) -> Result<Vec<String>, String> {
    use std::path::Path;

    use easel_platform::{select_wallpaper_backend, system_appearance};

    let appearance = system_appearance();
    let due = {
        let store = automation_store()?;
        store
            .due_dynamic_stills(now, utc_offset_minutes, appearance)
            .map_err(|error| error.to_string())?
    };
    if due.is_empty() {
        prerender_upcoming(now, utc_offset_minutes)?;
        return Ok(Vec::new());
    }

    let backend = select_wallpaper_backend().map_err(|error| error.to_string())?;
    let capabilities = backend.capabilities();
    let mut messages = Vec::new();

    for item in due {
        let (profile, still_set) = load_dynamic_profile(&item)?;
        if capabilities.native_dynamic_bundle {
            if let Some(message) = try_apply_native_dynamic_due(&profile, &still_set, &item)? {
                if !message.is_empty() {
                    messages.push(message);
                }
                continue;
            }
        }

        let path = resolve_asset_path(item.selection.asset_id)?;
        // Always apply from the original asset. Pre-rendered per-display crops must not be
        // fed back into `apply_still`, which re-composes for every active display.
        let apply_message = apply_service::apply_still(Path::new(&path), &profile)?;
        let fade_note = if item.request_cross_fade && !capabilities.cross_fade {
            "; cross-fade requested but unsupported (hard cut)"
        } else {
            ""
        };
        let reason = format!(
            "dynamic {}; {}{fade_note}",
            item.decision_reason, apply_message
        );
        record_dynamic_apply(&profile, &item, &reason)?;
        messages.push(format!(
            "dynamic '{}' → {} ({})",
            profile.name,
            item.selection.key_label(),
            item.selection.asset_id.to_hyphenated_string()
        ));
        if let Some(next_asset) = item.next_asset_id {
            let _ = prerender_asset(&profile, next_asset);
        }
    }
    Ok(messages)
}

fn load_dynamic_profile(
    item: &easel_scheduler::DueDynamicStill,
) -> Result<(Profile, easel_core::DynamicStillSet), String> {
    let store = automation_store()?;
    let profile = store
        .profile(item.profile_id)
        .cloned()
        .ok_or_else(|| "profile not found".to_string())?;
    let still_set = store
        .still_set(item.still_set_id)
        .cloned()
        .ok_or_else(|| "still set not found".to_string())?;
    Ok((profile, still_set))
}

/// Attempts native HEIC host apply. Returns `Ok(Some(msg))` when handled (skip still poller),
/// `Ok(None)` when the caller should fall back to still-frame apply.
///
/// `AlreadyHosting` updates state silently and returns `Some("")` so the caller skips the
/// still poller without appending an empty status line.
fn try_apply_native_dynamic_due(
    profile: &Profile,
    still_set: &easel_core::DynamicStillSet,
    item: &easel_scheduler::DueDynamicStill,
) -> Result<Option<String>, String> {
    use std::path::PathBuf;

    use crate::apply_service::{NativeDynamicApply, apply_native_dynamic};

    let Ok(frame_paths) = still_set
        .frames
        .iter()
        .map(|frame| resolve_asset_path(frame.asset_id).map(PathBuf::from))
        .collect::<Result<Vec<_>, _>>()
    else {
        return Ok(None);
    };

    match apply_native_dynamic(still_set, &frame_paths, profile, None) {
        Ok(NativeDynamicApply::AlreadyHosting { .. }) => {
            record_dynamic_state_only(item)?;
            Ok(Some(String::new()))
        }
        Ok(NativeDynamicApply::Applied { message, .. }) => {
            let reason = format!("dynamic native host; {message}");
            record_dynamic_apply(profile, item, &reason)?;
            Ok(Some(format!(
                "dynamic '{}' → native host ({})",
                profile.name,
                item.selection.key_label()
            )))
        }
        Err(_) => Ok(None),
    }
}

fn record_dynamic_state_only(item: &easel_scheduler::DueDynamicStill) -> Result<(), String> {
    use easel_core::AppliedDynamicFrame;
    use easel_scheduler::now_unix_i64;

    let occurred_at = now_unix_i64();
    let store = automation_store()?;
    store
        .history()
        .set_dynamic_still_state(
            item.profile_id,
            &AppliedDynamicFrame {
                asset_id: item.selection.asset_id,
                key_label: item.selection.key_label(),
                applied_at: occurred_at,
            },
        )
        .map_err(|error| error.to_string())
}

fn record_dynamic_apply(
    profile: &Profile,
    item: &easel_scheduler::DueDynamicStill,
    reason: &str,
) -> Result<(), String> {
    use easel_core::AppliedDynamicFrame;
    use easel_scheduler::{RotationHistoryEntry, now_unix_i64};

    let occurred_at = now_unix_i64();
    let store = automation_store()?;
    store
        .history()
        .record(&RotationHistoryEntry {
            queue_id: None,
            profile_id: item.profile_id,
            schedule_id: profile.schedule_id,
            asset_id: item.selection.asset_id,
            reason: reason.to_owned(),
            occurred_at,
        })
        .map_err(|error| error.to_string())?;
    store
        .history()
        .set_dynamic_still_state(
            item.profile_id,
            &AppliedDynamicFrame {
                asset_id: item.selection.asset_id,
                key_label: item.selection.key_label(),
                applied_at: occurred_at,
            },
        )
        .map_err(|error| error.to_string())
}

fn prerender_upcoming(now: InstantSeconds, utc_offset_minutes: i32) -> Result<(), String> {
    use easel_core::{PresentationMode, next_transition_after};

    let jobs: Vec<(easel_core::AssetId, Profile)> = {
        let store = automation_store()?;
        let mut jobs = Vec::new();
        for profile in store.profiles() {
            if profile.presentation != PresentationMode::DynamicStills {
                continue;
            }
            let Some(still_set_id) = profile.still_set_id else {
                continue;
            };
            let Some(still_set) = store.still_set(still_set_id) else {
                continue;
            };
            if let Some((_, frame)) = next_transition_after(still_set, now, utc_offset_minutes) {
                jobs.push((frame.asset_id, profile.clone()));
            }
        }
        jobs
    };
    for (asset_id, profile) in jobs {
        let _ = prerender_asset(&profile, asset_id);
    }
    Ok(())
}

fn prerender_asset(profile: &Profile, asset_id: AssetId) -> Result<(), String> {
    use std::path::Path;

    // Warm the ready slot with a copy of the original source. Multi-display composition
    // stays in `apply_still` so a single cropped PNG is never reused as a virtual desktop.
    let path = resolve_asset_path(asset_id)?;
    let store = automation_store()?;
    let staging = store.prerender_staging_path(profile.id, asset_id);
    let ready = store.prerender_ready_path(profile.id, asset_id);
    if ready.is_file() {
        return Ok(());
    }
    if let Some(parent) = staging.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::copy(Path::new(&path), &staging).map_err(|error| error.to_string())?;
    store
        .promote_prerender(&staging, &ready)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn resolve_asset_path(asset_id: AssetId) -> Result<String, String> {
    let library = library_store()?;
    let asset = library
        .get_asset(asset_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("asset not in library: {}", asset_id.to_hyphenated_string()))?;
    match asset.location {
        AssetLocation::Local { path } => Ok(path),
        AssetLocation::Remote { .. } => {
            Err("remote assets must be acquired before rotation".into())
        }
    }
}
