// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Versioned TOML persistence for profiles, groups, schedules, and queues.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use easel_core::{
    AssetId, DisplayGroup, DisplayGroupId, DynamicStillSet, DynamicStillSetId, HotplugPolicy,
    InstantSeconds, Profile, ProfileId, RotationQueue, RotationQueueId, Schedule, ScheduleId,
    ScheduleRule, active_frame_at, decide_transition, explain_fire, next_fire_after,
    next_transition_after, select_next, skip_current,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::history::{RotationHistoryStore, now_unix_i64};

/// On-disk locations for automation configuration and history.
#[derive(Clone, Debug)]
pub struct AutomationPaths {
    /// Directory holding TOML documents.
    pub config_dir: PathBuf,
    /// SQLite rotation history path.
    pub history_db: PathBuf,
}

impl AutomationPaths {
    /// Builds paths under a shared config/data root pair.
    #[must_use]
    pub fn new(config_dir: impl Into<PathBuf>, data_dir: impl Into<PathBuf>) -> Self {
        let config_dir = config_dir.into();
        let data_dir = data_dir.into();
        Self {
            history_db: data_dir.join("rotation_history.db"),
            config_dir,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ProfileDocument {
    profiles: Vec<Profile>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct GroupDocument {
    groups: Vec<DisplayGroup>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ScheduleDocument {
    schedules: Vec<Schedule>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct QueueDocument {
    queues: Vec<RotationQueue>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct StillSetDocument {
    still_sets: Vec<DynamicStillSet>,
}

/// Human-readable automation status for CLI and tray.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutomationSummary {
    /// Number of saved profiles.
    pub profile_count: usize,
    /// Number of enabled schedules.
    pub enabled_schedules: usize,
    /// Number of dynamic still sets.
    pub still_set_count: usize,
    /// Whether any rotation queue is paused.
    pub any_paused: bool,
    /// Next scheduled fire explanation, when known.
    pub next_fire_hint: Option<String>,
    /// Next dynamic-still transition hint, when known.
    pub next_dynamic_hint: Option<String>,
    /// Latest apply reason, when history exists.
    pub last_apply_reason: Option<String>,
    /// Hotplug policy label.
    pub hotplug_policy: String,
}

/// Loads and saves Stage 4/5 automation configuration.
pub struct AutomationStore {
    paths: AutomationPaths,
    history: RotationHistoryStore,
    profiles: Vec<Profile>,
    groups: Vec<DisplayGroup>,
    schedules: Vec<Schedule>,
    queues: Vec<RotationQueue>,
    still_sets: Vec<DynamicStillSet>,
    hotplug: HotplugPolicy,
}

impl AutomationStore {
    /// Opens configuration from `paths`, creating empty documents when missing.
    pub fn open(paths: AutomationPaths) -> Result<Self, AutomationStoreError> {
        fs::create_dir_all(&paths.config_dir)?;
        if let Some(parent) = paths.history_db.parent() {
            fs::create_dir_all(parent)?;
        }
        let history = RotationHistoryStore::open(&paths.history_db)?;
        let mut store = Self {
            paths,
            history,
            profiles: Vec::new(),
            groups: Vec::new(),
            schedules: Vec::new(),
            queues: Vec::new(),
            still_sets: Vec::new(),
            hotplug: HotplugPolicy::default(),
        };
        store.reload()?;
        Ok(store)
    }

    /// Returns configured filesystem paths.
    #[must_use]
    pub fn paths(&self) -> &AutomationPaths {
        &self.paths
    }

    /// Reloads all TOML documents from disk.
    pub fn reload(&mut self) -> Result<(), AutomationStoreError> {
        self.profiles = read_document::<ProfileDocument>(&self.profiles_path())?
            .unwrap_or_default()
            .profiles
            .into_iter()
            .map(|profile| {
                let migrated = profile.migrate()?;
                migrated.validate()?;
                Ok(migrated)
            })
            .collect::<Result<Vec<_>, AutomationStoreError>>()?;
        self.groups = read_document::<GroupDocument>(&self.groups_path())?
            .unwrap_or_default()
            .groups;
        for group in &self.groups {
            group.validate()?;
        }
        self.schedules = read_document::<ScheduleDocument>(&self.schedules_path())?
            .unwrap_or_default()
            .schedules;
        for schedule in &self.schedules {
            schedule.validate()?;
        }
        self.queues = read_document::<QueueDocument>(&self.queues_path())?
            .unwrap_or_default()
            .queues;
        for queue in &self.queues {
            queue.validate()?;
        }
        self.still_sets = read_document::<StillSetDocument>(&self.still_sets_path())?
            .unwrap_or_default()
            .still_sets;
        for still_set in &self.still_sets {
            still_set.validate()?;
        }
        self.hotplug = read_document::<HotplugPolicy>(&self.hotplug_path())?.unwrap_or_default();
        self.hotplug.validate()?;
        Ok(())
    }

    /// Saved profiles.
    #[must_use]
    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    /// Saved display groups.
    #[must_use]
    pub fn groups(&self) -> &[DisplayGroup] {
        &self.groups
    }

    /// Saved schedules.
    #[must_use]
    pub fn schedules(&self) -> &[Schedule] {
        &self.schedules
    }

    /// Saved rotation queues.
    #[must_use]
    pub fn queues(&self) -> &[RotationQueue] {
        &self.queues
    }

    /// Saved dynamic still sets.
    #[must_use]
    pub fn still_sets(&self) -> &[DynamicStillSet] {
        &self.still_sets
    }

    /// Active hotplug policy.
    #[must_use]
    pub fn hotplug_policy(&self) -> &HotplugPolicy {
        &self.hotplug
    }

    /// Rotation history store.
    #[must_use]
    pub fn history(&self) -> &RotationHistoryStore {
        &self.history
    }

    /// Upserts a profile and writes the profiles document.
    pub fn upsert_profile(&mut self, profile: Profile) -> Result<(), AutomationStoreError> {
        let profile = profile.migrate()?;
        profile.validate()?;
        if let Some(existing) = self.profiles.iter_mut().find(|item| item.id == profile.id) {
            *existing = profile;
        } else {
            self.profiles.push(profile);
        }
        self.write_profiles()
    }

    /// Deletes a profile by id and any schedules that reference it.
    pub fn delete_profile(&mut self, id: ProfileId) -> Result<(), AutomationStoreError> {
        self.profiles.retain(|profile| profile.id != id);
        let before = self.schedules.len();
        self.schedules.retain(|schedule| schedule.profile_id != id);
        let schedules_changed = self.schedules.len() != before;
        self.write_profiles()?;
        if schedules_changed {
            self.write_schedules()?;
        }
        Ok(())
    }

    /// Looks up a profile.
    #[must_use]
    pub fn profile(&self, id: ProfileId) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    /// Upserts a display group.
    pub fn upsert_group(&mut self, group: DisplayGroup) -> Result<(), AutomationStoreError> {
        group.validate()?;
        if let Some(existing) = self.groups.iter_mut().find(|item| item.id == group.id) {
            *existing = group;
        } else {
            self.groups.push(group);
        }
        self.write_groups()
    }

    /// Looks up a display group.
    #[must_use]
    pub fn group(&self, id: DisplayGroupId) -> Option<&DisplayGroup> {
        self.groups.iter().find(|group| group.id == id)
    }

    /// Upserts a schedule.
    pub fn upsert_schedule(&mut self, schedule: Schedule) -> Result<(), AutomationStoreError> {
        schedule.validate()?;
        if let Some(existing) = self
            .schedules
            .iter_mut()
            .find(|item| item.id == schedule.id)
        {
            *existing = schedule;
        } else {
            self.schedules.push(schedule);
        }
        self.write_schedules()
    }

    /// Looks up a schedule.
    #[must_use]
    pub fn schedule(&self, id: ScheduleId) -> Option<&Schedule> {
        self.schedules.iter().find(|schedule| schedule.id == id)
    }

    /// Upserts a rotation queue.
    pub fn upsert_queue(&mut self, queue: RotationQueue) -> Result<(), AutomationStoreError> {
        queue.validate()?;
        if let Some(existing) = self.queues.iter_mut().find(|item| item.id == queue.id) {
            *existing = queue;
        } else {
            self.queues.push(queue);
        }
        self.write_queues()
    }

    /// Looks up a rotation queue.
    #[must_use]
    pub fn queue(&self, id: RotationQueueId) -> Option<&RotationQueue> {
        self.queues.iter().find(|queue| queue.id == id)
    }

    /// Mutably looks up a rotation queue.
    pub fn queue_mut(&mut self, id: RotationQueueId) -> Option<&mut RotationQueue> {
        self.queues.iter_mut().find(|queue| queue.id == id)
    }

    /// Upserts a dynamic still set.
    pub fn upsert_still_set(
        &mut self,
        still_set: DynamicStillSet,
    ) -> Result<(), AutomationStoreError> {
        still_set.validate()?;
        if let Some(existing) = self
            .still_sets
            .iter_mut()
            .find(|item| item.id == still_set.id)
        {
            *existing = still_set;
        } else {
            self.still_sets.push(still_set);
        }
        self.write_still_sets()
    }

    /// Looks up a dynamic still set.
    #[must_use]
    pub fn still_set(&self, id: DynamicStillSetId) -> Option<&DynamicStillSet> {
        self.still_sets.iter().find(|still_set| still_set.id == id)
    }

    /// Replaces the hotplug policy.
    pub fn set_hotplug_policy(
        &mut self,
        policy: HotplugPolicy,
    ) -> Result<(), AutomationStoreError> {
        policy.validate()?;
        self.hotplug = policy;
        atomic_write_toml(&self.hotplug_path(), &self.hotplug)
    }

    /// Sets pause on every queue (global tray/CLI pause).
    pub fn set_all_paused(&mut self, paused: bool) -> Result<(), AutomationStoreError> {
        for queue in &mut self.queues {
            queue.policy.paused = paused;
        }
        self.write_queues()
    }

    /// Returns whether any queue is paused.
    #[must_use]
    pub fn any_paused(&self) -> bool {
        self.queues.iter().any(|queue| queue.policy.paused)
    }

    /// Builds a status summary.
    pub fn summary(
        &self,
        utc_offset_minutes: i32,
    ) -> Result<AutomationSummary, AutomationStoreError> {
        let now = InstantSeconds {
            unix_seconds: now_unix_i64(),
        };
        let mut next_hint = None;
        let mut best: Option<i64> = None;
        for schedule in &self.schedules {
            if !schedule.enabled {
                continue;
            }
            let last = self
                .history
                .last_fired(schedule.id)?
                .map(|unix_seconds| InstantSeconds { unix_seconds });
            if let Some(next) = next_fire_after(schedule, now, last, utc_offset_minutes) {
                if best.is_none_or(|current| next.unix_seconds < current) {
                    best = Some(next.unix_seconds);
                    next_hint = Some(format!(
                        "{}: {}",
                        schedule.name,
                        explain_fire(schedule, next, utc_offset_minutes)
                    ));
                }
            }
        }
        let last_apply_reason = self.history.latest()?.map(|entry| entry.reason);
        let mut next_dynamic_hint = None;
        let mut best_dynamic: Option<i64> = None;
        for profile in &self.profiles {
            if profile.presentation != easel_core::PresentationMode::DynamicStills {
                continue;
            }
            let Some(still_set_id) = profile.still_set_id else {
                continue;
            };
            let Some(still_set) = self.still_set(still_set_id) else {
                continue;
            };
            if let Some((instant, frame)) =
                next_transition_after(still_set, now, utc_offset_minutes)
            {
                if best_dynamic.is_none_or(|current| instant.unix_seconds < current) {
                    best_dynamic = Some(instant.unix_seconds);
                    next_dynamic_hint = Some(format!(
                        "{}: {} → {}",
                        profile.name,
                        frame.key.label(),
                        frame.asset_id.to_hyphenated_string()
                    ));
                }
            }
        }
        Ok(AutomationSummary {
            profile_count: self.profiles.len(),
            enabled_schedules: self
                .schedules
                .iter()
                .filter(|schedule| schedule.enabled)
                .count(),
            still_set_count: self.still_sets.len(),
            any_paused: self.any_paused(),
            next_fire_hint: next_hint,
            next_dynamic_hint,
            last_apply_reason,
            hotplug_policy: format!("{:?}", self.hotplug.on_missing),
        })
    }

    /// Selects the next asset for a profile's queue using avoid-repeat history.
    ///
    /// `membership` must already be resolved (explicit assets or collection members).
    pub fn select_for_profile(
        &self,
        profile_id: ProfileId,
        membership: &[AssetId],
    ) -> Result<(RotationQueueId, easel_core::SelectionDecision), AutomationStoreError> {
        let profile = self
            .profile(profile_id)
            .ok_or(AutomationStoreError::MissingProfile(profile_id))?;
        let queue_id = profile
            .rotation_queue_id
            .ok_or(AutomationStoreError::ProfileHasNoQueue(profile_id))?;
        let queue = self
            .queue(queue_id)
            .ok_or(AutomationStoreError::MissingQueue(queue_id))?;
        let recent = self
            .history
            .recent_assets(queue_id, queue.policy.avoid_repeat_count)?;
        let decision = select_next(queue, membership, &recent)?;
        Ok((queue_id, decision))
    }

    /// Skips the current queue candidate for a profile without applying.
    pub fn skip_for_profile(
        &mut self,
        profile_id: ProfileId,
        membership: &[AssetId],
    ) -> Result<(AssetId, String), AutomationStoreError> {
        let profile = self
            .profile(profile_id)
            .ok_or(AutomationStoreError::MissingProfile(profile_id))?
            .clone();
        let queue_id = profile
            .rotation_queue_id
            .ok_or(AutomationStoreError::ProfileHasNoQueue(profile_id))?;
        let queue = self
            .queue(queue_id)
            .ok_or(AutomationStoreError::MissingQueue(queue_id))?
            .clone();
        let (next_cursor, skipped) = skip_current(&queue, membership)?;
        if let Some(queue) = self.queue_mut(queue_id) {
            queue.cursor = next_cursor;
        }
        self.write_queues()?;
        Ok((
            skipped,
            format!(
                "skipped {} on queue {}",
                skipped.to_hyphenated_string(),
                queue.name
            ),
        ))
    }

    /// Persists an advanced queue cursor after a successful selection/apply.
    pub fn commit_selection(
        &mut self,
        queue_id: RotationQueueId,
        next_cursor: u32,
    ) -> Result<(), AutomationStoreError> {
        if let Some(queue) = self.queue_mut(queue_id) {
            queue.cursor = next_cursor;
        } else {
            return Err(AutomationStoreError::MissingQueue(queue_id));
        }
        self.write_queues()
    }

    /// Creates a schedule for `profile` from a Compose schedule index.
    ///
    /// 0 = Manual, 1 = Every hour, 2 = Time of day (09:00 and 18:00).
    pub fn schedule_from_compose_index(
        profile_id: ProfileId,
        name: impl Into<String>,
        index: i32,
    ) -> Result<Schedule, AutomationStoreError> {
        let name = name.into();
        let schedule = match index {
            0 => Schedule::manual(name, profile_id),
            1 => Schedule::interval(name, profile_id, 3600),
            2 => {
                let mut schedule = Schedule::manual(name, profile_id);
                schedule.rule = ScheduleRule::TimeOfDay {
                    times: vec![
                        easel_core::LocalTimeOfDay::new(9, 0)?,
                        easel_core::LocalTimeOfDay::new(18, 0)?,
                    ],
                };
                schedule
            }
            other => return Err(AutomationStoreError::UnknownScheduleIndex(other)),
        };
        schedule.validate()?;
        Ok(schedule)
    }

    /// Returns schedules that are due to fire at `now`.
    ///
    /// Non-interval (minute-granularity) rules are evaluated from a short look-back
    /// window so a 30s poller still catches `:00` slots that landed between ticks.
    /// Schedules whose `last_fired` is already at or after the candidate are skipped.
    pub fn due_schedules(
        &self,
        now: InstantSeconds,
        utc_offset_minutes: i32,
    ) -> Result<Vec<Schedule>, AutomationStoreError> {
        /// Seconds before `now` still considered for minute-granularity events.
        const DUE_GRACE_SECONDS: i64 = 60;

        let mut due = Vec::new();
        for schedule in &self.schedules {
            if !schedule.enabled {
                continue;
            }
            if matches!(&schedule.rule, ScheduleRule::Manual) {
                continue;
            }
            let last = self
                .history
                .last_fired(schedule.id)?
                .map(|unix_seconds| InstantSeconds { unix_seconds });
            let window_start = InstantSeconds {
                unix_seconds: now.unix_seconds.saturating_sub(DUE_GRACE_SECONDS),
            };
            if let Some(candidate) =
                next_fire_after(schedule, window_start, last, utc_offset_minutes)
            {
                if candidate.unix_seconds > now.unix_seconds {
                    continue;
                }
                if last.is_some_and(|fired| fired.unix_seconds >= candidate.unix_seconds) {
                    continue;
                }
                due.push(schedule.clone());
            }
        }
        Ok(due)
    }

    /// Profiles whose dynamic still frame should be applied at `now`.
    ///
    /// Skipped while any rotation queue is paused (shared tray/CLI pause).
    pub fn due_dynamic_stills(
        &self,
        now: InstantSeconds,
        utc_offset_minutes: i32,
    ) -> Result<Vec<DueDynamicStill>, AutomationStoreError> {
        if self.any_paused() {
            return Ok(Vec::new());
        }
        let mut due = Vec::new();
        for profile in &self.profiles {
            if profile.presentation != easel_core::PresentationMode::DynamicStills {
                continue;
            }
            let Some(still_set_id) = profile.still_set_id else {
                continue;
            };
            let Some(still_set) = self.still_set(still_set_id) else {
                return Err(AutomationStoreError::MissingStillSet(still_set_id));
            };
            let selection = active_frame_at(still_set, now, utc_offset_minutes);
            let last = self.history.dynamic_still_state(profile.id)?;
            let decision = decide_transition(last.as_ref(), &selection);
            if !decision.should_apply {
                continue;
            }
            let next = next_transition_after(still_set, now, utc_offset_minutes);
            due.push(DueDynamicStill {
                profile_id: profile.id,
                still_set_id,
                selection,
                decision_reason: decision.reason,
                next_transition_unix: next.as_ref().map(|(instant, _)| instant.unix_seconds),
                next_asset_id: next.map(|(_, frame)| frame.asset_id),
                request_cross_fade: still_set.request_cross_fade,
            });
        }
        Ok(due)
    }

    /// Staging directory for pre-rendered dynamic still outputs.
    #[must_use]
    pub fn prerender_dir(&self) -> PathBuf {
        self.paths
            .history_db
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
            .join("dynamic-prerender")
    }

    /// Staging path for a profile's upcoming dynamic frame.
    #[must_use]
    pub fn prerender_staging_path(&self, profile_id: ProfileId, asset_id: AssetId) -> PathBuf {
        self.prerender_dir().join(format!(
            "{}-{}.staging.png",
            profile_id.to_hyphenated_string(),
            asset_id.to_hyphenated_string()
        ))
    }

    /// Completed path for a profile's upcoming dynamic frame.
    #[must_use]
    pub fn prerender_ready_path(&self, profile_id: ProfileId, asset_id: AssetId) -> PathBuf {
        self.prerender_dir().join(format!(
            "{}-{}.png",
            profile_id.to_hyphenated_string(),
            asset_id.to_hyphenated_string()
        ))
    }

    /// Atomically promotes a completed staging render into the ready path.
    pub fn promote_prerender(
        &self,
        staging: &Path,
        ready: &Path,
    ) -> Result<(), AutomationStoreError> {
        if let Some(parent) = ready.parent() {
            fs::create_dir_all(parent)?;
        }
        if !staging.is_file() {
            return Err(AutomationStoreError::MissingPrerender(
                staging.to_path_buf(),
            ));
        }
        let temp = ready.with_extension("png.part");
        fs::copy(staging, &temp)?;
        {
            let file = fs::File::open(&temp)?;
            file.sync_all()?;
        }
        fs::rename(&temp, ready)?;
        let _ = fs::remove_file(staging);
        Ok(())
    }

    fn profiles_path(&self) -> PathBuf {
        self.paths.config_dir.join("profiles.toml")
    }

    fn groups_path(&self) -> PathBuf {
        self.paths.config_dir.join("display_groups.toml")
    }

    fn schedules_path(&self) -> PathBuf {
        self.paths.config_dir.join("schedules.toml")
    }

    fn queues_path(&self) -> PathBuf {
        self.paths.config_dir.join("rotation_queues.toml")
    }

    fn still_sets_path(&self) -> PathBuf {
        self.paths.config_dir.join("dynamic_still_sets.toml")
    }

    fn hotplug_path(&self) -> PathBuf {
        self.paths.config_dir.join("hotplug.toml")
    }

    fn write_profiles(&self) -> Result<(), AutomationStoreError> {
        atomic_write_toml(
            &self.profiles_path(),
            &ProfileDocument {
                profiles: self.profiles.clone(),
            },
        )
    }

    fn write_groups(&self) -> Result<(), AutomationStoreError> {
        atomic_write_toml(
            &self.groups_path(),
            &GroupDocument {
                groups: self.groups.clone(),
            },
        )
    }

    fn write_schedules(&self) -> Result<(), AutomationStoreError> {
        atomic_write_toml(
            &self.schedules_path(),
            &ScheduleDocument {
                schedules: self.schedules.clone(),
            },
        )
    }

    fn write_queues(&self) -> Result<(), AutomationStoreError> {
        atomic_write_toml(
            &self.queues_path(),
            &QueueDocument {
                queues: self.queues.clone(),
            },
        )
    }

    fn write_still_sets(&self) -> Result<(), AutomationStoreError> {
        atomic_write_toml(
            &self.still_sets_path(),
            &StillSetDocument {
                still_sets: self.still_sets.clone(),
            },
        )
    }
}

/// A dynamic still profile that needs an apply at the evaluated instant.
#[derive(Clone, Debug, PartialEq)]
pub struct DueDynamicStill {
    /// Profile to update.
    pub profile_id: ProfileId,
    /// Still set driving the selection.
    pub still_set_id: DynamicStillSetId,
    /// Frame that should be showing.
    pub selection: easel_core::FrameSelection,
    /// Why the transition should apply.
    pub decision_reason: String,
    /// Next transition unix seconds, when known.
    pub next_transition_unix: Option<i64>,
    /// Asset that becomes active at the next transition.
    pub next_asset_id: Option<AssetId>,
    /// Whether the still set requested a cross-fade.
    pub request_cross_fade: bool,
}

fn read_document<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<Option<T>, AutomationStoreError> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    let value = toml::from_str(&text)?;
    Ok(Some(value))
}

fn atomic_write_toml<T: Serialize>(path: &Path, value: &T) -> Result<(), AutomationStoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string_pretty(value)?;
    let temp = path.with_extension("toml.part");
    let stash = path.with_extension("toml.bak");
    {
        let mut file = fs::File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    let had_existing = path.exists();
    if had_existing {
        let _ = fs::remove_file(&stash);
        if let Err(error) = fs::rename(path, &stash) {
            let _ = fs::remove_file(&temp);
            return Err(error.into());
        }
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
            Err(error.into())
        }
    }
}

/// Automation store failures.
#[derive(Debug, Error)]
pub enum AutomationStoreError {
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// TOML serialization failure.
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
    /// TOML parse failure.
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    /// Profile validation failure.
    #[error(transparent)]
    Profile(#[from] easel_core::ProfileValidationError),
    /// Display group validation failure.
    #[error(transparent)]
    Group(#[from] easel_core::DisplayGroupError),
    /// Schedule validation failure.
    #[error(transparent)]
    Schedule(#[from] easel_core::ScheduleError),
    /// Rotation validation/selection failure.
    #[error(transparent)]
    Rotation(#[from] easel_core::RotationError),
    /// Hotplug validation failure.
    #[error(transparent)]
    Hotplug(#[from] easel_core::HotplugError),
    /// Dynamic still set validation failure.
    #[error(transparent)]
    DynamicStill(#[from] easel_core::DynamicStillError),
    /// History store failure.
    #[error(transparent)]
    History(#[from] crate::history::RotationHistoryStoreError),
    /// Referenced profile is missing.
    #[error("profile not found: {0:?}")]
    MissingProfile(ProfileId),
    /// Referenced queue is missing.
    #[error("rotation queue not found: {0:?}")]
    MissingQueue(RotationQueueId),
    /// Referenced dynamic still set is missing.
    #[error("dynamic still set not found: {0:?}")]
    MissingStillSet(DynamicStillSetId),
    /// Profile has no rotation queue assigned.
    #[error("profile has no rotation queue: {0:?}")]
    ProfileHasNoQueue(ProfileId),
    /// Compose schedule combo index is unknown.
    #[error("unknown compose schedule index: {0}")]
    UnknownScheduleIndex(i32),
    /// Pre-rendered staging file is missing.
    #[error("missing dynamic pre-render: {0}")]
    MissingPrerender(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::{DisplayId, RotationQueue};

    fn temp_paths() -> (AutomationPaths, PathBuf) {
        let root = std::env::temp_dir().join(format!("easel-auto-{}", uuid::Uuid::new_v4()));
        let paths = AutomationPaths::new(root.join("config"), root.join("data"));
        (paths, root)
    }

    #[test]
    fn persists_profile_and_schedule() {
        let (paths, root) = temp_paths();
        let mut store = AutomationStore::open(paths.clone()).unwrap();
        let mut profile = Profile::new("Desk");
        let displays = vec![DisplayId::from_u128(1)];
        profile.displays = displays.clone();
        let queue = RotationQueue::from_assets("Queue", vec![AssetId::new()]);
        profile.rotation_queue_id = Some(queue.id);
        let schedule =
            AutomationStore::schedule_from_compose_index(profile.id, "Hourly", 1).unwrap();
        profile.schedule_id = Some(schedule.id);
        store.upsert_queue(queue).unwrap();
        store.upsert_profile(profile.clone()).unwrap();
        store.upsert_schedule(schedule).unwrap();

        let reloaded = AutomationStore::open(paths).unwrap();
        assert_eq!(reloaded.profiles().len(), 1);
        assert_eq!(reloaded.schedules().len(), 1);
        assert_eq!(reloaded.queues().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pause_blocks_selection() {
        let (paths, root) = temp_paths();
        let mut store = AutomationStore::open(paths).unwrap();
        let asset = AssetId::new();
        let queue = RotationQueue::from_assets("Queue", vec![asset]);
        let mut profile = Profile::new("Desk");
        profile.rotation_queue_id = Some(queue.id);
        let queue_id = queue.id;
        store.upsert_queue(queue).unwrap();
        store.upsert_profile(profile.clone()).unwrap();
        store.set_all_paused(true).unwrap();
        let err = store.select_for_profile(profile.id, &[asset]).unwrap_err();
        assert!(matches!(
            err,
            AutomationStoreError::Rotation(easel_core::RotationError::Paused)
        ));
        let _ = queue_id;
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn delete_profile_removes_orphan_schedules() {
        let (paths, root) = temp_paths();
        let mut store = AutomationStore::open(paths.clone()).unwrap();
        let profile = Profile::new("Desk");
        let schedule =
            AutomationStore::schedule_from_compose_index(profile.id, "Hourly", 1).unwrap();
        store.upsert_profile(profile.clone()).unwrap();
        store.upsert_schedule(schedule).unwrap();
        assert_eq!(store.schedules().len(), 1);
        store.delete_profile(profile.id).unwrap();
        assert!(store.profiles().is_empty());
        assert!(store.schedules().is_empty());
        let reloaded = AutomationStore::open(paths).unwrap();
        assert!(reloaded.profiles().is_empty());
        assert!(reloaded.schedules().is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn due_schedules_uses_grace_window_and_last_fired_guard() {
        let (paths, root) = temp_paths();
        let mut store = AutomationStore::open(paths).unwrap();
        let profile = Profile::new("Desk");
        let mut schedule = Schedule::manual("Nine", profile.id);
        schedule.rule = ScheduleRule::TimeOfDay {
            times: vec![easel_core::LocalTimeOfDay::new(9, 0).unwrap()],
        };
        schedule.enabled = true;
        store.upsert_profile(profile).unwrap();
        store.upsert_schedule(schedule.clone()).unwrap();

        // 09:00:30 UTC on 2024-01-01 — within the 60s grace of 09:00:00.
        let now = InstantSeconds {
            unix_seconds: 1_704_099_630,
        };
        let due = store.due_schedules(now, 0).unwrap();
        assert_eq!(due.len(), 1);

        store
            .history()
            .set_last_fired(schedule.id, 1_704_099_600)
            .unwrap();
        let due_again = store.due_schedules(now, 0).unwrap();
        assert!(
            due_again.is_empty(),
            "already-fired schedule must not re-trigger"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn persists_and_evaluates_dynamic_still_set() {
        let (paths, root) = temp_paths();
        let mut store = AutomationStore::open(paths.clone()).unwrap();
        let asset_a = AssetId::new();
        let asset_b = AssetId::new();
        let mut profile = Profile::new("Dynamic");
        profile.presentation = easel_core::PresentationMode::DynamicStills;
        let mut still_set =
            easel_core::DynamicStillSet::default_time_of_day("Day", profile.id, asset_a).unwrap();
        still_set.frames[1].asset_id = asset_b;
        profile.still_set_id = Some(still_set.id);
        store.upsert_still_set(still_set).unwrap();
        store.upsert_profile(profile.clone()).unwrap();

        let reloaded = AutomationStore::open(paths).unwrap();
        assert_eq!(reloaded.still_sets().len(), 1);
        // 2024-01-01 14:30 UTC → noon frame
        let now = InstantSeconds {
            unix_seconds: 1_704_119_400,
        };
        let due = reloaded.due_dynamic_stills(now, 0).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].selection.asset_id, asset_b);
        assert_eq!(due[0].selection.key_label(), "tod:12:00");

        reloaded
            .history()
            .set_dynamic_still_state(
                profile.id,
                &easel_core::AppliedDynamicFrame {
                    asset_id: asset_b,
                    key_label: "tod:12:00".into(),
                    applied_at: now.unix_seconds,
                },
            )
            .unwrap();
        let due_again = reloaded.due_dynamic_stills(now, 0).unwrap();
        assert!(due_again.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pause_blocks_dynamic_still_due() {
        let (paths, root) = temp_paths();
        let mut store = AutomationStore::open(paths).unwrap();
        let asset = AssetId::new();
        let mut profile = Profile::new("Dynamic");
        profile.presentation = easel_core::PresentationMode::DynamicStills;
        let still_set =
            easel_core::DynamicStillSet::default_time_of_day("Day", profile.id, asset).unwrap();
        profile.still_set_id = Some(still_set.id);
        let queue = RotationQueue::from_assets("Queue", vec![asset]);
        store.upsert_queue(queue).unwrap();
        store.upsert_still_set(still_set).unwrap();
        store.upsert_profile(profile).unwrap();
        store.set_all_paused(true).unwrap();
        let now = InstantSeconds {
            unix_seconds: 1_704_119_400,
        };
        assert!(store.due_dynamic_stills(now, 0).unwrap().is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
