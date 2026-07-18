// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Time-of-day and solar-keyed dynamic still sets.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::schedule::{
    InstantSeconds, LocalTimeOfDay, SolarEvent, instant_at_local, solar_event_local_minutes,
};
use crate::{AssetId, ProfileId};

/// Current serialized dynamic-still-set schema.
pub const DYNAMIC_STILL_SET_SCHEMA_VERSION: u16 = 1;

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

/// When a still frame becomes active within a day.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
        }
    }
}

/// One keyed frame inside a dynamic still set.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DynamicStillFrame {
    /// When this frame becomes active.
    pub key: DynamicStillKey,
    /// Still asset shown while this frame is active.
    pub asset_id: AssetId,
}

/// Ordered time/solar still set with a required fallback frame.
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
    /// Keyed frames; evaluation uses resolved instants, not declaration order.
    pub frames: Vec<DynamicStillFrame>,
    /// Asset used when no keyed frame can be resolved (polar night, empty day, etc.).
    pub fallback_asset_id: AssetId,
    /// Observer latitude for solar keys (−90..=90).
    pub latitude_deg: f64,
    /// Observer longitude for solar keys (−180..=180).
    pub longitude_deg: f64,
    /// Request a cross-fade when the active backend supports it without a live host.
    pub request_cross_fade: bool,
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
            frames: Vec::new(),
            fallback_asset_id,
            latitude_deg: 40.7128,
            longitude_deg: -74.0060,
            request_cross_fade: false,
        }
    }

    /// Builds a default morning / noon / evening time-of-day set from one asset.
    pub fn default_time_of_day(
        name: impl Into<String>,
        profile_id: ProfileId,
        asset_id: AssetId,
    ) -> Result<Self, DynamicStillError> {
        let mut set = Self::with_fallback(name, profile_id, asset_id);
        set.frames = vec![
            DynamicStillFrame {
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(6, 0)?,
                },
                asset_id,
            },
            DynamicStillFrame {
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(12, 0)?,
                },
                asset_id,
            },
            DynamicStillFrame {
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(18, 0)?,
                },
                asset_id,
            },
        ];
        set.validate()?;
        Ok(set)
    }

    /// Validates schema and frame invariants.
    pub fn validate(&self) -> Result<(), DynamicStillError> {
        if self.schema_version != DYNAMIC_STILL_SET_SCHEMA_VERSION {
            return Err(DynamicStillError::UnsupportedSchema(self.schema_version));
        }
        if self.name.trim().is_empty() {
            return Err(DynamicStillError::EmptyName);
        }
        if !(-90.0..=90.0).contains(&self.latitude_deg) || !self.latitude_deg.is_finite() {
            return Err(DynamicStillError::InvalidLatitude(self.latitude_deg));
        }
        if !(-180.0..=180.0).contains(&self.longitude_deg) || !self.longitude_deg.is_finite() {
            return Err(DynamicStillError::InvalidLongitude(self.longitude_deg));
        }
        let mut seen = Vec::with_capacity(self.frames.len());
        for frame in &self.frames {
            if let DynamicStillKey::TimeOfDay { time } = frame.key {
                LocalTimeOfDay::new(time.hour, time.minute)?;
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
#[derive(Clone, Debug, Eq, PartialEq)]
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

/// Selects the active frame at `now` using a fixed UTC offset and solar observer.
#[must_use]
pub fn active_frame_at(
    set: &DynamicStillSet,
    now: InstantSeconds,
    utc_offset_minutes: i32,
) -> FrameSelection {
    let mut best: Option<(InstantSeconds, &DynamicStillFrame)> = None;
    // Look back across yesterday so early-morning evaluation still finds last night's key.
    for day_offset in -1..=0 {
        for frame in &set.frames {
            if let Some(instant) =
                resolve_key_on_day(set, frame.key, now, utc_offset_minutes, day_offset)
            {
                if instant.unix_seconds <= now.unix_seconds
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
        let local = instant.to_local(utc_offset_minutes);
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

    FrameSelection {
        asset_id: set.fallback_asset_id,
        key: None,
        reason: "fallback frame (no keyed transition at or before now)".into(),
        used_fallback: true,
    }
}

/// Returns the next keyed transition strictly after `now`, when one exists.
#[must_use]
pub fn next_transition_after(
    set: &DynamicStillSet,
    now: InstantSeconds,
    utc_offset_minutes: i32,
) -> Option<(InstantSeconds, DynamicStillFrame)> {
    let mut best: Option<(InstantSeconds, DynamicStillFrame)> = None;
    for day_offset in 0..=2 {
        for frame in &set.frames {
            if let Some(instant) =
                resolve_key_on_day(set, frame.key, now, utc_offset_minutes, day_offset)
            {
                if instant.unix_seconds > now.unix_seconds
                    && best
                        .as_ref()
                        .is_none_or(|(current, _)| instant.unix_seconds < current.unix_seconds)
                {
                    best = Some((instant, frame.clone()));
                }
            }
        }
    }
    best
}

/// Decides whether to apply `selection` given the last successful dynamic apply.
///
/// Catch-up after sleep or a forward clock jump applies the current frame once.
/// A backward clock jump that lands on a different key also applies once.
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

fn resolve_key_on_day(
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
            let ordinal = day_of_year(year, month, day);
            let solar_minutes =
                solar_event_local_minutes(ordinal, set.latitude_deg, set.longitude_deg, event)?;
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
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(6, 0).unwrap(),
                },
                asset_id: asset_a,
            },
            DynamicStillFrame {
                key: DynamicStillKey::TimeOfDay {
                    time: LocalTimeOfDay::new(12, 0).unwrap(),
                },
                asset_id: asset_b,
            },
            DynamicStillFrame {
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
        // 2024-01-01 14:30 UTC
        let now = InstantSeconds {
            unix_seconds: 1_704_119_400,
        };
        let selection = active_frame_at(&set, now, 0);
        assert_eq!(selection.asset_id, b);
        assert!(!selection.used_fallback);
        assert_eq!(selection.key_label(), "tod:12:00");
    }

    #[test]
    fn uses_fallback_before_first_key() {
        let a = AssetId::new();
        let set = morning_noon_evening(a, a, a);
        // 2024-01-01 03:00 UTC — before 06:00
        let now = InstantSeconds {
            unix_seconds: 1_704_078_000,
        };
        // Looking back to yesterday 18:00 should win over fallback.
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
            unix_seconds: 1_704_119_400, // 14:30
        };
        let (instant, frame) = next_transition_after(&set, now, 0).unwrap();
        assert_eq!(frame.key.label(), "tod:18:00");
        assert!(instant.unix_seconds > now.unix_seconds);
    }

    #[test]
    fn missed_transition_applies_once_on_catch_up() {
        let a = AssetId::new();
        let b = AssetId::new();
        let set = morning_noon_evening(a, b, a);
        let now = InstantSeconds {
            unix_seconds: 1_704_119_400, // 14:30 → noon frame
        };
        let selection = active_frame_at(&set, now, 0);
        let last = AppliedDynamicFrame {
            asset_id: a,
            key_label: "tod:06:00".into(),
            applied_at: now.unix_seconds - 8 * 3600,
        };
        let decision = decide_transition(Some(&last), &selection);
        assert!(decision.should_apply);
        let again = decide_transition(
            Some(&AppliedDynamicFrame {
                asset_id: selection.asset_id,
                key_label: selection.key_label(),
                applied_at: now.unix_seconds,
            }),
            &selection,
        );
        assert!(!again.should_apply);
    }

    #[test]
    fn solar_key_resolves_near_equator() {
        let asset = AssetId::new();
        let mut set = DynamicStillSet::with_fallback("Solar", ProfileId::new(), asset);
        set.latitude_deg = 0.0;
        set.longitude_deg = 0.0;
        set.frames = vec![DynamicStillFrame {
            key: DynamicStillKey::Solar {
                event: SolarEvent::Sunrise,
                offset_minutes: 0,
            },
            asset_id: asset,
        }];
        set.validate().unwrap();
        // 2024-03-20 noon UTC
        let now = InstantSeconds {
            unix_seconds: 1_710_936_000,
        };
        let selection = active_frame_at(&set, now, 0);
        assert!(!selection.used_fallback);
        assert_eq!(selection.asset_id, asset);
    }

    #[test]
    fn rejects_duplicate_keys() {
        let asset = AssetId::new();
        let mut set = DynamicStillSet::with_fallback("Dup", ProfileId::new(), asset);
        let time = LocalTimeOfDay::new(8, 0).unwrap();
        set.frames = vec![
            DynamicStillFrame {
                key: DynamicStillKey::TimeOfDay { time },
                asset_id: asset,
            },
            DynamicStillFrame {
                key: DynamicStillKey::TimeOfDay { time },
                asset_id: asset,
            },
        ];
        assert!(matches!(
            set.validate(),
            Err(DynamicStillError::DuplicateKey(_))
        ));
    }
}
