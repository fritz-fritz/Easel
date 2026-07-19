// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Dynamic still sets keyed by time, solar position, or appearance.
//!
//! The portable domain mirrors Apple Dynamic Desktop HEIC metadata (`solar`,
//! `apr`, `h24`) so importers and native bundle exporters share one model.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::schedule::{
    InstantSeconds, LocalTimeOfDay, SolarEvent, instant_at_local, solar_event_local_minutes,
    solar_position_deg,
};
use crate::{AssetId, ProfileId};

/// Current serialized dynamic-still-set schema.
pub const DYNAMIC_STILL_SET_SCHEMA_VERSION: u16 = 2;

/// Stable identity for a dynamic still set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DynamicStillSetId(Uuid);

impl DynamicStillSetId {
    /// Creates a new still-set identity.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Returns the canonical hyphenated UUID string.
    #[must_use]
    pub fn to_hyphenated_string(self) -> String {
        self.0.hyphenated().to_string()
    }

    /// Parses a hyphenated UUID string.
    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(value.trim())?))
    }
}

impl Default for DynamicStillSetId {
    fn default() -> Self {
        Self::new()
    }
}

/// Which evaluation rule a still set uses (matches Apple HEIC metadata flavors).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DynamicScheduleKind {
    /// Wall-clock samples (`apple_desktop:h24` / authored ToD / sunrise-sunset).
    #[default]
    TimeOfDay,
    /// Nearest sun altitude/azimuth sample (`apple_desktop:solar`).
    SolarPosition,
    /// Light/dark appearance (`apple_desktop:apr`).
    Appearance,
}

/// Light or dark appearance selection for appearance-keyed sets.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppearanceMode {
    /// Light / day appearance.
    #[default]
    Light,
    /// Dark / night appearance.
    Dark,
}

/// When a still frame becomes active.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DynamicStillKey {
    /// Local wall-clock time.
    TimeOfDay {
        /// Local hour and minute.
        time: LocalTimeOfDay,
    },
    /// Approximate sunrise or sunset plus an offset.
    Solar {
        /// Sunrise or sunset.
        event: SolarEvent,
        /// Minutes added after the solar event (may be negative).
        offset_minutes: i32,
    },
    /// Sample keyed to sun altitude and azimuth (Apple `solar` metadata).
    SolarPosition {
        /// Solar altitude / elevation in degrees (negative below horizon).
        altitude_deg: f64,
        /// Solar azimuth in degrees (0..=360, north-based convention as stored).
        azimuth_deg: f64,
    },
    /// Appearance-mode frame (Apple `apr` metadata).
    Appearance {
        /// Light or dark.
        mode: AppearanceMode,
    },
}

impl DynamicStillKey {
    /// Stable label used in history and UI.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::TimeOfDay { time } => format!("tod:{:02}:{:02}", time.hour, time.minute),
            Self::Solar {
                event,
                offset_minutes,
            } => {
                let name = match event {
                    SolarEvent::Sunrise => "sunrise",
                    SolarEvent::Sunset => "sunset",
                };
                format!("solar:{name}{offset_minutes:+}m")
            }
            Self::SolarPosition {
                altitude_deg,
                azimuth_deg,
            } => format!("sun:a{altitude_deg:.3}:z{azimuth_deg:.3}"),
            Self::Appearance { mode } => match mode {
                AppearanceMode::Light => "appearance:light".into(),
                AppearanceMode::Dark => "appearance:dark".into(),
            },
        }
    }
}

/// One keyed frame inside a dynamic still set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DynamicStillFrame {
    /// Optional HEIC image index for round-trip with Apple / Plasma packages.
    #[serde(default)]
    pub source_index: Option<u32>,
    /// When this frame becomes active.
    pub key: DynamicStillKey,
    /// Still asset shown while this frame is active.
    pub asset_id: AssetId,
}

/// Ordered time/solar/appearance still set with a required fallback frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DynamicStillSet {
    /// Serialized schema version.
    pub schema_version: u16,
    /// Stable identity.
    pub id: DynamicStillSetId,
    /// User-visible name.
    pub name: String,
    /// Profile that owns this still set (for diagnostics).
    pub profile_id: ProfileId,
    /// Evaluation rule for this set.
    #[serde(default)]
    pub schedule_kind: DynamicScheduleKind,
    /// Keyed frames; evaluation uses the active `schedule_kind`.
    pub frames: Vec<DynamicStillFrame>,
    /// Asset used when no keyed frame can be resolved.
    pub fallback_asset_id: AssetId,
    /// Observer latitude for solar keys (−90..=90). `None` until the user sets a location.
    #[serde(default)]
    pub latitude_deg: Option<f64>,
    /// Observer longitude for solar keys (−180..=180). `None` until the user sets a location.
    #[serde(default)]
    pub longitude_deg: Option<f64>,
    /// Request a cross-fade when the active backend supports it without a live host.
    pub request_cross_fade: bool,
    /// Optional path to the original dynamic HEIC / Plasma package for re-export.
    #[serde(default)]
    pub source_package_path: Option<String>,
}

/// Injected context for appearance and solar evaluation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicEvalContext {
    /// Current instant.
    pub now: InstantSeconds,
    /// Fixed local offset east of UTC in minutes.
    pub utc_offset_minutes: i32,
    /// Current light/dark preference for appearance-keyed sets.
    pub appearance: AppearanceMode,
}

impl DynamicStillSet {
    /// Creates a still set with a single fallback and no keyed frames.
    #[must_use]
    pub fn with_fallback(
        name: impl Into<String>,
        profile_id: ProfileId,
        fallback_asset_id: AssetId,
    ) -> Self {
        Self {
            schema_version: DYNAMIC_STILL_SET_SCHEMA_VERSION,
            id: DynamicStillSetId::new(),
            name: name.into(),
            profile_id,
            schedule_kind: DynamicScheduleKind::TimeOfDay,
            frames: Vec::new(),
            fallback_asset_id,
            latitude_deg: None,
            longitude_deg: None,
            request_cross_fade: false,
            source_package_path: None,
        }
    }

    /// Returns configured observer coordinates when both latitude and longitude are set.
    #[must_use]
    pub fn observer_location(&self) -> Option<(f64, f64)> {
        match (self.latitude_deg, self.longitude_deg) {
            (Some(latitude_deg), Some(longitude_deg)) => Some((latitude_deg, longitude_deg)),
            _ => None,
        }
    }

    /// Builds a dense hourly time-of-day set from one asset (placeholder before HEIC import).
    pub fn default_hourly(
        name: impl Into<String>,
        profile_id: ProfileId,
        asset_id: AssetId,
    ) -> Result<Self, DynamicStillError> {
        let mut set = Self::with_fallback(name, profile_id, asset_id);
        set.schedule_kind = DynamicScheduleKind::TimeOfDay;
        set.frames = (0..24)
            .map(|hour| {
                Ok(DynamicStillFrame {
                    source_index: Some(u32::from(hour)),
                    key: DynamicStillKey::TimeOfDay {
                        time: LocalTimeOfDay::new(hour, 0)?,
                    },
                    asset_id,
                })
            })
            .collect::<Result<Vec<_>, DynamicStillError>>()?;
        set.validate()?;
        Ok(set)
    }

    /// Legacy three-slot helper retained for older Compose saves.
    pub fn default_time_of_day(
        name: impl Into<String>,
        profile_id: ProfileId,
        asset_id: AssetId,
    ) -> Result<Self, DynamicStillError> {
        let mut set = Self::with_fallback(name, profile_id, asset_id);
        set.frames = vec![
            DynamicStillFrame {
                source_index: Some(0),
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(6, 0)?,
                },
                asset_id,
            },
            DynamicStillFrame {
                source_index: Some(1),
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(12, 0)?,
                },
                asset_id,
            },
            DynamicStillFrame {
                source_index: Some(2),
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(18, 0)?,
                },
                asset_id,
            },
        ];
        set.validate()?;
        Ok(set)
    }

    /// Upgrades older on-disk still sets to the current schema.
    pub fn migrate(mut self) -> Result<Self, DynamicStillError> {
        match self.schema_version {
            1 => {
                self.schema_version = DYNAMIC_STILL_SET_SCHEMA_VERSION;
                if self
                    .frames
                    .iter()
                    .any(|frame| matches!(frame.key, DynamicStillKey::SolarPosition { .. }))
                {
                    self.schedule_kind = DynamicScheduleKind::SolarPosition;
                } else if self
                    .frames
                    .iter()
                    .any(|frame| matches!(frame.key, DynamicStillKey::Appearance { .. }))
                {
                    self.schedule_kind = DynamicScheduleKind::Appearance;
                } else {
                    self.schedule_kind = DynamicScheduleKind::TimeOfDay;
                }
                Ok(self)
            }
            DYNAMIC_STILL_SET_SCHEMA_VERSION => Ok(self),
            other => Err(DynamicStillError::UnsupportedSchema(other)),
        }
    }

    /// Validates schema and frame invariants.
    pub fn validate(&self) -> Result<(), DynamicStillError> {
        if self.schema_version != DYNAMIC_STILL_SET_SCHEMA_VERSION {
            return Err(DynamicStillError::UnsupportedSchema(self.schema_version));
        }
        if self.name.trim().is_empty() {
            return Err(DynamicStillError::EmptyName);
        }
        if let Some(latitude_deg) = self.latitude_deg {
            if !(-90.0..=90.0).contains(&latitude_deg) || !latitude_deg.is_finite() {
                return Err(DynamicStillError::InvalidLatitude(latitude_deg));
            }
        }
        if let Some(longitude_deg) = self.longitude_deg {
            if !(-180.0..=180.0).contains(&longitude_deg) || !longitude_deg.is_finite() {
                return Err(DynamicStillError::InvalidLongitude(longitude_deg));
            }
        }
        if self.latitude_deg.is_some() != self.longitude_deg.is_some() {
            return Err(DynamicStillError::IncompleteObserver);
        }
        let mut seen = Vec::with_capacity(self.frames.len());
        for frame in &self.frames {
            match frame.key {
                DynamicStillKey::TimeOfDay { time } => {
                    LocalTimeOfDay::new(time.hour, time.minute)?;
                }
                DynamicStillKey::SolarPosition {
                    altitude_deg,
                    azimuth_deg,
                } => {
                    if !altitude_deg.is_finite() || !(-90.0..=90.0).contains(&altitude_deg) {
                        return Err(DynamicStillError::InvalidSolarPosition {
                            altitude_deg,
                            azimuth_deg,
                        });
                    }
                    if !azimuth_deg.is_finite() || !(0.0..360.0).contains(&azimuth_deg) {
                        return Err(DynamicStillError::InvalidSolarPosition {
                            altitude_deg,
                            azimuth_deg,
                        });
                    }
                }
                DynamicStillKey::Solar { .. } | DynamicStillKey::Appearance { .. } => {}
            }
            let label = frame.key.label();
            if seen.contains(&label) {
                return Err(DynamicStillError::DuplicateKey(label));
            }
            seen.push(label);
        }
        Ok(())
    }
}

/// Result of evaluating which frame should be showing.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameSelection {
    /// Asset to render and apply.
    pub asset_id: AssetId,
    /// Key that won, when a keyed frame is active.
    pub key: Option<DynamicStillKey>,
    /// Explainable selection reason.
    pub reason: String,
    /// True when the fallback asset was chosen.
    pub used_fallback: bool,
}

impl FrameSelection {
    /// Stable fingerprint for last-applied comparisons.
    #[must_use]
    pub fn key_label(&self) -> String {
        self.key
            .map_or_else(|| "fallback".into(), DynamicStillKey::label)
    }
}

/// Previously applied dynamic-still frame for catch-up decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedDynamicFrame {
    /// Asset that was applied.
    pub asset_id: AssetId,
    /// Key label (`fallback` or `DynamicStillKey::label`).
    pub key_label: String,
    /// Unix seconds when the apply completed.
    pub applied_at: i64,
}

/// Whether a newly evaluated selection should replace the desktop wallpaper.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionDecision {
    /// Apply when true.
    pub should_apply: bool,
    /// Explainable reason.
    pub reason: String,
}

/// Selects the active frame using the still set's `schedule_kind`.
#[must_use]
pub fn active_frame_at(
    set: &DynamicStillSet,
    now: InstantSeconds,
    utc_offset_minutes: i32,
) -> FrameSelection {
    active_frame_with_context(
        set,
        DynamicEvalContext {
            now,
            utc_offset_minutes,
            appearance: AppearanceMode::Light,
        },
    )
}

/// Selects the active frame with an explicit appearance preference.
#[must_use]
pub fn active_frame_with_context(set: &DynamicStillSet, ctx: DynamicEvalContext) -> FrameSelection {
    match set.schedule_kind {
        DynamicScheduleKind::Appearance => active_appearance_frame(set, ctx.appearance),
        DynamicScheduleKind::SolarPosition => active_solar_position_frame(set, ctx),
        DynamicScheduleKind::TimeOfDay => active_time_keyed_frame(set, ctx),
    }
}

/// Returns the next keyed transition strictly after `now`, when one exists.
///
/// Solar-position sets do not expose a cheap closed-form next fire; callers should
/// re-evaluate periodically. Appearance sets never auto-transition from the clock.
#[must_use]
pub fn next_transition_after(
    set: &DynamicStillSet,
    now: InstantSeconds,
    utc_offset_minutes: i32,
) -> Option<(InstantSeconds, DynamicStillFrame)> {
    match set.schedule_kind {
        DynamicScheduleKind::Appearance | DynamicScheduleKind::SolarPosition => None,
        DynamicScheduleKind::TimeOfDay => {
            let mut best: Option<(InstantSeconds, DynamicStillFrame)> = None;
            for day_offset in 0..=2 {
                for frame in &set.frames {
                    if let Some(instant) =
                        resolve_time_key_on_day(set, frame.key, now, utc_offset_minutes, day_offset)
                    {
                        if instant.unix_seconds > now.unix_seconds
                            && best.as_ref().is_none_or(|(current, _)| {
                                instant.unix_seconds < current.unix_seconds
                            })
                        {
                            best = Some((instant, frame.clone()));
                        }
                    }
                }
            }
            best
        }
    }
}

/// Decides whether to apply `selection` given the last successful dynamic apply.
#[must_use]
pub fn decide_transition(
    last: Option<&AppliedDynamicFrame>,
    selection: &FrameSelection,
) -> TransitionDecision {
    match last {
        None => TransitionDecision {
            should_apply: true,
            reason: format!("initial dynamic apply ({})", selection.key_label()),
        },
        Some(previous) => {
            let same_key = previous.key_label == selection.key_label();
            let same_asset = previous.asset_id == selection.asset_id;
            if same_key && same_asset {
                TransitionDecision {
                    should_apply: false,
                    reason: format!("already showing {}", selection.key_label()),
                }
            } else {
                TransitionDecision {
                    should_apply: true,
                    reason: format!(
                        "transition {} → {} ({})",
                        previous.key_label,
                        selection.key_label(),
                        selection.reason
                    ),
                }
            }
        }
    }
}

/// Angular distance between two solar samples (degrees), with azimuth wrap.
#[must_use]
pub fn solar_sample_distance(
    altitude_a: f64,
    azimuth_a: f64,
    altitude_b: f64,
    azimuth_b: f64,
) -> f64 {
    let d_alt = altitude_a - altitude_b;
    let mut d_az = (azimuth_a - azimuth_b).abs();
    if d_az > 180.0 {
        d_az = 360.0 - d_az;
    }
    let mean_alt = (altitude_a + altitude_b) * 0.5;
    let az_scale = mean_alt.to_radians().cos().abs().max(0.2);
    (d_alt * d_alt + (d_az * az_scale) * (d_az * az_scale)).sqrt()
}

fn active_time_keyed_frame(set: &DynamicStillSet, ctx: DynamicEvalContext) -> FrameSelection {
    let mut best: Option<(InstantSeconds, &DynamicStillFrame)> = None;
    for day_offset in -1..=0 {
        for frame in &set.frames {
            if let Some(instant) =
                resolve_time_key_on_day(set, frame.key, ctx.now, ctx.utc_offset_minutes, day_offset)
            {
                if instant.unix_seconds <= ctx.now.unix_seconds
                    && best
                        .as_ref()
                        .is_none_or(|(current, _)| instant.unix_seconds >= current.unix_seconds)
                {
                    best = Some((instant, frame));
                }
            }
        }
    }

    if let Some((instant, frame)) = best {
        let local = instant.to_local(ctx.utc_offset_minutes);
        return FrameSelection {
            asset_id: frame.asset_id,
            key: Some(frame.key),
            reason: format!(
                "active {} since {:02}:{:02} local",
                frame.key.label(),
                local.time.hour,
                local.time.minute
            ),
            used_fallback: false,
        };
    }

    fallback_selection(set, "fallback frame (no keyed transition at or before now)")
}

fn active_solar_position_frame(set: &DynamicStillSet, ctx: DynamicEvalContext) -> FrameSelection {
    let Some((latitude_deg, longitude_deg)) = set.observer_location() else {
        return fallback_selection(
            set,
            "fallback frame (observer location unset — set latitude/longitude for solar evaluation)",
        );
    };
    let (altitude, azimuth) =
        solar_position_deg(ctx.now, ctx.utc_offset_minutes, latitude_deg, longitude_deg);
    let mut best: Option<(f64, &DynamicStillFrame)> = None;
    for frame in &set.frames {
        let DynamicStillKey::SolarPosition {
            altitude_deg,
            azimuth_deg,
        } = frame.key
        else {
            continue;
        };
        let distance = solar_sample_distance(altitude, azimuth, altitude_deg, azimuth_deg);
        if best.is_none_or(|(current, _)| distance < current) {
            best = Some((distance, frame));
        }
    }
    if let Some((distance, frame)) = best {
        return FrameSelection {
            asset_id: frame.asset_id,
            key: Some(frame.key),
            reason: format!(
                "nearest solar sample {} (Δ={distance:.2}°, sun a={altitude:.1}° z={azimuth:.1}°)",
                frame.key.label()
            ),
            used_fallback: false,
        };
    }
    fallback_selection(set, "fallback frame (no solar-position samples)")
}

fn active_appearance_frame(set: &DynamicStillSet, mode: AppearanceMode) -> FrameSelection {
    if let Some(frame) = set.frames.iter().find(|frame| {
        matches!(
            frame.key,
            DynamicStillKey::Appearance {
                mode: frame_mode
            } if frame_mode == mode
        )
    }) {
        return FrameSelection {
            asset_id: frame.asset_id,
            key: Some(frame.key),
            reason: format!("appearance {mode:?}"),
            used_fallback: false,
        };
    }
    fallback_selection(set, "fallback frame (appearance mode missing)")
}

fn fallback_selection(set: &DynamicStillSet, reason: &str) -> FrameSelection {
    FrameSelection {
        asset_id: set.fallback_asset_id,
        key: None,
        reason: reason.into(),
        used_fallback: true,
    }
}

fn resolve_time_key_on_day(
    set: &DynamicStillSet,
    key: DynamicStillKey,
    now: InstantSeconds,
    utc_offset_minutes: i32,
    day_offset: i32,
) -> Option<InstantSeconds> {
    let local = now.to_local(utc_offset_minutes);
    let (year, month, day) = add_days(local.year, local.month, local.day, day_offset);
    match key {
        DynamicStillKey::TimeOfDay { time } => {
            Some(instant_at_local(year, month, day, time, utc_offset_minutes))
        }
        DynamicStillKey::Solar {
            event,
            offset_minutes,
        } => {
            let (latitude_deg, longitude_deg) = set.observer_location()?;
            let ordinal = day_of_year(year, month, day);
            let solar_minutes =
                solar_event_local_minutes(ordinal, latitude_deg, longitude_deg, event)?;
            let total = solar_minutes + offset_minutes;
            let wrapped_day = total.div_euclid(24 * 60);
            let minutes_in_day = total.rem_euclid(24 * 60);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let hour = (minutes_in_day / 60) as u8;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let minute = (minutes_in_day % 60) as u8;
            let (y, m, d) = add_days(year, month, day, wrapped_day);
            let time = LocalTimeOfDay { hour, minute };
            Some(instant_at_local(y, m, d, time, utc_offset_minutes))
        }
        DynamicStillKey::SolarPosition { .. } | DynamicStillKey::Appearance { .. } => None,
    }
}

fn add_days(year: i32, month: u8, day: u8, delta: i32) -> (i32, u8, u8) {
    let days = days_from_civil(year, month, day) + i64::from(delta);
    civil_ymd_from_days(days)
}

fn day_of_year(year: i32, month: u8, day: u8) -> u16 {
    let jan1 = days_from_civil(year, 1, 1);
    let today = days_from_civil(year, month, day);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (today - jan1 + 1) as u16
    }
}

#[allow(
    clippy::similar_names,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn civil_ymd_from_days(days: i64) -> (i32, u8, u8) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u8, d as u8)
}

#[allow(clippy::similar_names)]
fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let y = i64::from(if month <= 2 { year - 1 } else { year });
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = i64::from(if month > 2 { month - 3 } else { month + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Invalid dynamic still set or evaluation input.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum DynamicStillError {
    /// No migration exists for the serialized schema.
    #[error("unsupported dynamic still set schema version: {0}")]
    UnsupportedSchema(u16),
    /// Names must contain visible characters.
    #[error("dynamic still set name cannot be empty")]
    EmptyName,
    /// Latitude must be finite and within ±90°.
    #[error("invalid latitude: {0}")]
    InvalidLatitude(f64),
    /// Longitude must be finite and within ±180°.
    #[error("invalid longitude: {0}")]
    InvalidLongitude(f64),
    /// Latitude and longitude must both be set or both omitted.
    #[error("observer latitude and longitude must both be set")]
    IncompleteObserver,
    /// Solar position sample is out of range.
    #[error("invalid solar position altitude={altitude_deg} azimuth={azimuth_deg}")]
    InvalidSolarPosition {
        /// Altitude that failed validation.
        altitude_deg: f64,
        /// Azimuth that failed validation.
        azimuth_deg: f64,
    },
    /// Two frames share the same key label.
    #[error("duplicate dynamic still key: {0}")]
    DuplicateKey(String),
    /// Embedded time-of-day key is invalid.
    #[error(transparent)]
    Schedule(#[from] crate::ScheduleError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn morning_noon_evening(
        asset_a: AssetId,
        asset_b: AssetId,
        asset_c: AssetId,
    ) -> DynamicStillSet {
        let mut set = DynamicStillSet::with_fallback("Day", ProfileId::new(), asset_a);
        set.frames = vec![
            DynamicStillFrame {
                source_index: Some(0),
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(6, 0).unwrap(),
                },
                asset_id: asset_a,
            },
            DynamicStillFrame {
                source_index: Some(1),
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(12, 0).unwrap(),
                },
                asset_id: asset_b,
            },
            DynamicStillFrame {
                source_index: Some(2),
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(18, 0).unwrap(),
                },
                asset_id: asset_c,
            },
        ];
        set.validate().unwrap();
        set
    }

    #[test]
    fn selects_latest_keyed_frame_at_or_before_now() {
        let a = AssetId::new();
        let b = AssetId::new();
        let c = AssetId::new();
        let set = morning_noon_evening(a, b, c);
        let now = InstantSeconds {
            unix_seconds: 1_704_119_400,
        };
        let selection = active_frame_at(&set, now, 0);
        assert_eq!(selection.asset_id, b);
        assert!(!selection.used_fallback);
        assert_eq!(selection.key_label(), "tod:12:00");
    }

    #[test]
    fn uses_yesterday_evening_before_first_key() {
        let a = AssetId::new();
        let set = morning_noon_evening(a, a, a);
        let now = InstantSeconds {
            unix_seconds: 1_704_078_000,
        };
        let selection = active_frame_at(&set, now, 0);
        assert!(!selection.used_fallback);
        assert_eq!(selection.key_label(), "tod:18:00");
    }

    #[test]
    fn empty_frames_use_fallback() {
        let asset = AssetId::new();
        let set = DynamicStillSet::with_fallback("Empty", ProfileId::new(), asset);
        let now = InstantSeconds {
            unix_seconds: 1_704_100_000,
        };
        let selection = active_frame_at(&set, now, 0);
        assert!(selection.used_fallback);
        assert_eq!(selection.asset_id, asset);
    }

    #[test]
    fn next_transition_finds_upcoming_key() {
        let a = AssetId::new();
        let set = morning_noon_evening(a, a, a);
        let now = InstantSeconds {
            unix_seconds: 1_704_119_400,
        };
        let (instant, frame) = next_transition_after(&set, now, 0).unwrap();
        assert_eq!(frame.key.label(), "tod:18:00");
        assert!(instant.unix_seconds > now.unix_seconds);
    }

    #[test]
    fn solar_position_picks_nearest_sample() {
        let dawn = AssetId::new();
        let noon = AssetId::new();
        let dusk = AssetId::new();
        let mut set = DynamicStillSet::with_fallback("Solar", ProfileId::new(), dawn);
        set.schedule_kind = DynamicScheduleKind::SolarPosition;
        set.latitude_deg = Some(0.0);
        set.longitude_deg = Some(0.0);
        set.frames = vec![
            DynamicStillFrame {
                source_index: Some(0),
                key: DynamicStillKey::SolarPosition {
                    altitude_deg: -10.0,
                    azimuth_deg: 90.0,
                },
                asset_id: dawn,
            },
            DynamicStillFrame {
                source_index: Some(1),
                key: DynamicStillKey::SolarPosition {
                    altitude_deg: 60.0,
                    azimuth_deg: 180.0,
                },
                asset_id: noon,
            },
            DynamicStillFrame {
                source_index: Some(2),
                key: DynamicStillKey::SolarPosition {
                    altitude_deg: -5.0,
                    azimuth_deg: 270.0,
                },
                asset_id: dusk,
            },
        ];
        set.validate().unwrap();
        // 2024-03-20 noon UTC near equator → high altitude.
        let now = InstantSeconds {
            unix_seconds: 1_710_936_000,
        };
        let selection = active_frame_at(&set, now, 0);
        assert_eq!(selection.asset_id, noon);
    }

    #[test]
    fn appearance_selects_dark_frame() {
        let light = AssetId::new();
        let dark = AssetId::new();
        let mut set = DynamicStillSet::with_fallback("Apr", ProfileId::new(), light);
        set.schedule_kind = DynamicScheduleKind::Appearance;
        set.frames = vec![
            DynamicStillFrame {
                source_index: Some(0),
                key: DynamicStillKey::Appearance {
                    mode: AppearanceMode::Light,
                },
                asset_id: light,
            },
            DynamicStillFrame {
                source_index: Some(1),
                key: DynamicStillKey::Appearance {
                    mode: AppearanceMode::Dark,
                },
                asset_id: dark,
            },
        ];
        set.validate().unwrap();
        let selection = active_frame_with_context(
            &set,
            DynamicEvalContext {
                now: InstantSeconds {
                    unix_seconds: 1_704_100_000,
                },
                utc_offset_minutes: 0,
                appearance: AppearanceMode::Dark,
            },
        );
        assert_eq!(selection.asset_id, dark);
        assert_eq!(selection.key_label(), "appearance:dark");
    }

    #[test]
    fn hourly_default_has_twenty_four_frames() {
        let asset = AssetId::new();
        let set = DynamicStillSet::default_hourly("Day", ProfileId::new(), asset).unwrap();
        assert_eq!(set.frames.len(), 24);
        assert_eq!(set.schedule_kind, DynamicScheduleKind::TimeOfDay);
    }

    #[test]
    fn migrate_v1_infers_solar_position_kind() {
        let asset = AssetId::new();
        let mut set = DynamicStillSet::with_fallback("Old", ProfileId::new(), asset);
        set.schema_version = 1;
        set.frames = vec![DynamicStillFrame {
            source_index: Some(0),
            key: DynamicStillKey::SolarPosition {
                altitude_deg: 10.0,
                azimuth_deg: 120.0,
            },
            asset_id: asset,
        }];
        let migrated = set.migrate().unwrap();
        assert_eq!(migrated.schema_version, 2);
        assert_eq!(migrated.schedule_kind, DynamicScheduleKind::SolarPosition);
    }
}
