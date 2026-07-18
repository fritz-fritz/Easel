// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wall-clock, interval, solar, and calendar schedule rules.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::ProfileId;

/// Current serialized schedule schema.
pub const SCHEDULE_SCHEMA_VERSION: u16 = 1;

/// Stable schedule identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ScheduleId(Uuid);

impl ScheduleId {
    /// Creates a new schedule identity.
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

impl Default for ScheduleId {
    fn default() -> Self {
        Self::new()
    }
}

/// Wall-clock local time of day (hour and minute).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct LocalTimeOfDay {
    /// Hour in 0..=23.
    pub hour: u8,
    /// Minute in 0..=59.
    pub minute: u8,
}

impl LocalTimeOfDay {
    /// Creates a validated local time.
    pub fn new(hour: u8, minute: u8) -> Result<Self, ScheduleError> {
        if hour > 23 || minute > 59 {
            return Err(ScheduleError::InvalidTimeOfDay { hour, minute });
        }
        Ok(Self { hour, minute })
    }

    /// Minutes since local midnight.
    #[must_use]
    pub fn minutes_since_midnight(self) -> u32 {
        u32::from(self.hour) * 60 + u32::from(self.minute)
    }
}

/// Which solar event a schedule fires on.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolarEvent {
    /// Civil sunrise approximation.
    Sunrise,
    /// Civil sunset approximation.
    Sunset,
}

/// How and when a profile should rotate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleRule {
    /// Manual apply only; the scheduler never auto-fires.
    Manual,
    /// Fire every `every_seconds` after the previous successful apply.
    Interval {
        /// Seconds between rotations; must be at least 60.
        every_seconds: u64,
    },
    /// Fire at each listed local wall-clock time each day.
    TimeOfDay {
        /// Distinct local times, sorted ascending after validation.
        times: Vec<LocalTimeOfDay>,
    },
    /// Fire at approximate sunrise or sunset plus an offset.
    SunriseSunset {
        /// Sunrise or sunset.
        event: SolarEvent,
        /// Minutes added after the solar event (may be negative).
        offset_minutes: i32,
        /// Observer latitude in degrees (−90..=90).
        latitude_deg: f64,
        /// Observer longitude in degrees (−180..=180).
        longitude_deg: f64,
    },
    /// Fire on selected weekdays at one local time.
    Calendar {
        /// Bitmask: bit 0 = Monday … bit 6 = Sunday.
        weekdays: u8,
        /// Local fire time.
        time: LocalTimeOfDay,
    },
}

/// A named schedule bound to one wallpaper profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    /// Serialized schema version.
    pub schema_version: u16,
    /// Stable identity.
    pub id: ScheduleId,
    /// User-visible name.
    pub name: String,
    /// Profile that receives selected assets when this schedule fires.
    pub profile_id: ProfileId,
    /// Whether the automation runtime may fire this schedule.
    pub enabled: bool,
    /// Timing rule.
    pub rule: ScheduleRule,
}

impl Schedule {
    /// Creates an enabled manual schedule for `profile_id`.
    #[must_use]
    pub fn manual(name: impl Into<String>, profile_id: ProfileId) -> Self {
        Self {
            schema_version: SCHEDULE_SCHEMA_VERSION,
            id: ScheduleId::new(),
            name: name.into(),
            profile_id,
            enabled: true,
            rule: ScheduleRule::Manual,
        }
    }

    /// Creates an interval schedule.
    #[must_use]
    pub fn interval(name: impl Into<String>, profile_id: ProfileId, every_seconds: u64) -> Self {
        Self {
            schema_version: SCHEDULE_SCHEMA_VERSION,
            id: ScheduleId::new(),
            name: name.into(),
            profile_id,
            enabled: true,
            rule: ScheduleRule::Interval { every_seconds },
        }
    }

    /// Validates schema and rule invariants.
    pub fn validate(&self) -> Result<(), ScheduleError> {
        if self.schema_version != SCHEDULE_SCHEMA_VERSION {
            return Err(ScheduleError::UnsupportedSchema(self.schema_version));
        }
        if self.name.trim().is_empty() {
            return Err(ScheduleError::EmptyName);
        }
        match &self.rule {
            ScheduleRule::Manual => Ok(()),
            ScheduleRule::Interval { every_seconds } => {
                if *every_seconds < 60 {
                    Err(ScheduleError::IntervalTooShort(*every_seconds))
                } else {
                    Ok(())
                }
            }
            ScheduleRule::TimeOfDay { times } => {
                if times.is_empty() {
                    return Err(ScheduleError::EmptyTimeList);
                }
                let mut seen = Vec::with_capacity(times.len());
                for time in times {
                    time.validate()?;
                    if seen.contains(time) {
                        return Err(ScheduleError::DuplicateTimeOfDay(*time));
                    }
                    seen.push(*time);
                }
                Ok(())
            }
            ScheduleRule::SunriseSunset {
                latitude_deg,
                longitude_deg,
                ..
            } => {
                if !(-90.0..=90.0).contains(latitude_deg) || !latitude_deg.is_finite() {
                    return Err(ScheduleError::InvalidLatitude(*latitude_deg));
                }
                if !(-180.0..=180.0).contains(longitude_deg) || !longitude_deg.is_finite() {
                    return Err(ScheduleError::InvalidLongitude(*longitude_deg));
                }
                Ok(())
            }
            ScheduleRule::Calendar { weekdays, time } => {
                if *weekdays == 0 || (*weekdays & !0b0111_1111) != 0 {
                    return Err(ScheduleError::InvalidWeekdays(*weekdays));
                }
                time.validate()
            }
        }
    }
}

impl LocalTimeOfDay {
    fn validate(self) -> Result<(), ScheduleError> {
        Self::new(self.hour, self.minute).map(|_| ())
    }
}

/// Instant used for deterministic schedule evaluation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstantSeconds {
    /// Unix epoch seconds (UTC).
    pub unix_seconds: i64,
}

/// Local civil calendar fields derived from an instant plus a fixed UTC offset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalCivilTime {
    /// Year (proleptic Gregorian).
    pub year: i32,
    /// Month 1..=12.
    pub month: u8,
    /// Day of month 1..=31.
    pub day: u8,
    /// ISO weekday where Monday = 0 and Sunday = 6.
    pub weekday: u8,
    /// Local time of day.
    pub time: LocalTimeOfDay,
    /// Day-of-year 1..=366.
    pub day_of_year: u16,
}

impl InstantSeconds {
    /// Converts this UTC instant into local civil fields using a fixed offset.
    #[must_use]
    pub fn to_local(self, utc_offset_minutes: i32) -> LocalCivilTime {
        let local = self.unix_seconds + i64::from(utc_offset_minutes) * 60;
        let day_seconds = ((local % 86_400) + 86_400) % 86_400;
        let days = (local - day_seconds) / 86_400;
        let (year, month, day, day_of_year) = civil_from_days(days);
        // 1970-01-01 was Thursday; Monday=0 → Thursday=3.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let weekday = ((days + 3).rem_euclid(7)) as u8;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let hour = (day_seconds / 3600) as u8;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let minute = ((day_seconds % 3600) / 60) as u8;
        LocalCivilTime {
            year,
            month,
            day,
            weekday,
            time: LocalTimeOfDay { hour, minute },
            day_of_year,
        }
    }
}

/// Returns the next fire instant at or after `now`, when the schedule is enabled.
///
/// `last_fired` is required for interval rules; other rules ignore it.
/// `utc_offset_minutes` is the observer's fixed offset from UTC.
#[must_use]
pub fn next_fire_after(
    schedule: &Schedule,
    now: InstantSeconds,
    last_fired: Option<InstantSeconds>,
    utc_offset_minutes: i32,
) -> Option<InstantSeconds> {
    if !schedule.enabled {
        return None;
    }
    match &schedule.rule {
        ScheduleRule::Manual => None,
        ScheduleRule::Interval { every_seconds } => {
            let base = last_fired.unwrap_or(now).unix_seconds;
            let next = if last_fired.is_some() {
                base.saturating_add(i64::try_from(*every_seconds).unwrap_or(i64::MAX))
            } else {
                now.unix_seconds
            };
            Some(InstantSeconds {
                unix_seconds: next.max(now.unix_seconds),
            })
        }
        ScheduleRule::TimeOfDay { times } => {
            Some(next_daily_local_times(now, utc_offset_minutes, times))
        }
        ScheduleRule::SunriseSunset {
            event,
            offset_minutes,
            latitude_deg,
            longitude_deg,
        } => next_solar_event(
            now,
            utc_offset_minutes,
            *event,
            *offset_minutes,
            *latitude_deg,
            *longitude_deg,
        ),
        ScheduleRule::Calendar { weekdays, time } => {
            next_calendar_fire(now, utc_offset_minutes, *weekdays, *time)
        }
    }
}

/// Explains why a schedule would fire at `candidate` (for diagnostics and UI).
#[must_use]
pub fn explain_fire(
    schedule: &Schedule,
    candidate: InstantSeconds,
    utc_offset_minutes: i32,
) -> String {
    let local = candidate.to_local(utc_offset_minutes);
    match &schedule.rule {
        ScheduleRule::Manual => "manual schedule never auto-fires".into(),
        ScheduleRule::Interval { every_seconds } => {
            format!("interval every {every_seconds}s")
        }
        ScheduleRule::TimeOfDay { .. } => format!(
            "time-of-day at {:02}:{:02} local",
            local.time.hour, local.time.minute
        ),
        ScheduleRule::SunriseSunset {
            event,
            offset_minutes,
            ..
        } => {
            let label = match event {
                SolarEvent::Sunrise => "sunrise",
                SolarEvent::Sunset => "sunset",
            };
            format!(
                "{label}{offset_minutes:+}m → {:02}:{:02} local",
                local.time.hour, local.time.minute
            )
        }
        ScheduleRule::Calendar { weekdays, time } => {
            format!(
                "calendar weekdays={weekdays:#010b} at {:02}:{:02}",
                time.hour, time.minute
            )
        }
    }
}

fn next_daily_local_times(
    now: InstantSeconds,
    utc_offset_minutes: i32,
    times: &[LocalTimeOfDay],
) -> InstantSeconds {
    let mut sorted = times.to_vec();
    sorted.sort_unstable();
    let local = now.to_local(utc_offset_minutes);
    // Compare full instants (second precision) rather than truncated minutes so a
    // candidate at HH:MM:00 is not treated as still due after HH:MM:01.
    for time in &sorted {
        let candidate = instant_at_local(
            local.year,
            local.month,
            local.day,
            *time,
            utc_offset_minutes,
        );
        if candidate.unix_seconds >= now.unix_seconds {
            return candidate;
        }
    }
    let (year, month, day) = add_days(local.year, local.month, local.day, 1);
    instant_at_local(year, month, day, sorted[0], utc_offset_minutes)
}

fn next_calendar_fire(
    now: InstantSeconds,
    utc_offset_minutes: i32,
    weekdays: u8,
    time: LocalTimeOfDay,
) -> Option<InstantSeconds> {
    let local = now.to_local(utc_offset_minutes);
    for offset in 0..8 {
        let (year, month, day) = add_days(local.year, local.month, local.day, offset);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let weekday = ((i64::from(local.weekday) + i64::from(offset)).rem_euclid(7)) as u8;
        if (weekdays & (1 << weekday)) == 0 {
            continue;
        }
        let candidate = instant_at_local(year, month, day, time, utc_offset_minutes);
        if candidate.unix_seconds >= now.unix_seconds {
            return Some(candidate);
        }
    }
    None
}

fn next_solar_event(
    now: InstantSeconds,
    utc_offset_minutes: i32,
    event: SolarEvent,
    offset_minutes: i32,
    latitude_deg: f64,
    longitude_deg: f64,
) -> Option<InstantSeconds> {
    let local = now.to_local(utc_offset_minutes);
    for day_offset in 0..3 {
        let (year, month, day) = add_days(local.year, local.month, local.day, day_offset);
        let ordinal = day_of_year(year, month, day);
        let solar_minutes = solar_event_local_minutes(ordinal, latitude_deg, longitude_deg, event)?;
        let total = solar_minutes + offset_minutes;
        let wrapped_day = total.div_euclid(24 * 60);
        let minutes_in_day = total.rem_euclid(24 * 60);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let hour = (minutes_in_day / 60) as u8;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let minute = (minutes_in_day % 60) as u8;
        let (y, m, d) = add_days(year, month, day, wrapped_day);
        let time = LocalTimeOfDay { hour, minute };
        let candidate = instant_at_local(y, m, d, time, utc_offset_minutes);
        if candidate.unix_seconds >= now.unix_seconds {
            return Some(candidate);
        }
    }
    None
}

/// NOAA-style approximate local solar event minutes past local midnight.
///
/// Returns `None` when the event does not occur (for example polar night/day).
#[allow(
    clippy::unreadable_literal,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
#[must_use]
pub fn solar_event_local_minutes(
    day_of_year: u16,
    latitude_deg: f64,
    longitude_deg: f64,
    event: SolarEvent,
) -> Option<i32> {
    let lat = latitude_deg.to_radians();
    let gamma = 2.0 * std::f64::consts::PI / 365.0 * (f64::from(day_of_year) - 1.0);
    let eq_time = 229.18
        * (0.000075 + 0.001868 * gamma.cos()
            - 0.032077 * gamma.sin()
            - 0.014615 * (2.0 * gamma).cos()
            - 0.040849 * (2.0 * gamma).sin());
    let decl = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin()
        - 0.006758 * (2.0 * gamma).cos()
        + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos()
        + 0.00148 * (3.0 * gamma).sin();
    let cos_ha =
        (90.833_f64.to_radians().cos() - lat.sin() * decl.sin()) / (lat.cos() * decl.cos());
    if !(-1.0..=1.0).contains(&cos_ha) {
        return None;
    }
    let ha = cos_ha.acos().to_degrees();
    let solar_noon = 720.0 - 4.0 * longitude_deg - eq_time;
    let minutes = match event {
        SolarEvent::Sunrise => solar_noon - ha * 4.0,
        SolarEvent::Sunset => solar_noon + ha * 4.0,
    };
    Some(minutes.round() as i32)
}

/// Converts a local civil date and time into a UTC instant using a fixed offset.
#[must_use]
pub fn instant_at_local(
    year: i32,
    month: u8,
    day: u8,
    time: LocalTimeOfDay,
    utc_offset_minutes: i32,
) -> InstantSeconds {
    let days = days_from_civil(year, month, day);
    let local_seconds = days * 86_400 + i64::from(time.hour) * 3600 + i64::from(time.minute) * 60;
    InstantSeconds {
        unix_seconds: local_seconds - i64::from(utc_offset_minutes) * 60,
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

fn civil_from_days(days: i64) -> (i32, u8, u8, u16) {
    let (year, month, day) = civil_ymd_from_days(days);
    let ordinal = day_of_year(year, month, day);
    (year, month, day, ordinal)
}

/// Howard Hinnant public-domain civil-from-days.
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

/// Invalid schedule model or rule.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum ScheduleError {
    /// No migration exists for the serialized schema.
    #[error("unsupported schedule schema version: {0}")]
    UnsupportedSchema(u16),
    /// Schedule names must contain visible characters.
    #[error("schedule name cannot be empty")]
    EmptyName,
    /// Interval schedules must be at least one minute.
    #[error("interval schedule must be at least 60 seconds, got {0}")]
    IntervalTooShort(u64),
    /// Time-of-day schedules need at least one fire time.
    #[error("time-of-day schedule requires at least one time")]
    EmptyTimeList,
    /// Hour/minute out of range.
    #[error("invalid time of day {hour:02}:{minute:02}")]
    InvalidTimeOfDay {
        /// Invalid hour.
        hour: u8,
        /// Invalid minute.
        minute: u8,
    },
    /// Duplicate fire times are rejected.
    #[error("duplicate time of day {0:?}")]
    DuplicateTimeOfDay(LocalTimeOfDay),
    /// Latitude out of range.
    #[error("latitude must be between -90 and 90, got {0}")]
    InvalidLatitude(f64),
    /// Longitude out of range.
    #[error("longitude must be between -180 and 180, got {0}")]
    InvalidLongitude(f64),
    /// Weekday bitmask must select at least one day and use only bits 0..=6.
    #[error("invalid weekday bitmask: {0:#010b}")]
    InvalidWeekdays(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_next_fire_uses_last_fired() {
        let profile = ProfileId::new();
        let schedule = Schedule::interval("Hourly", profile, 3600);
        let now = InstantSeconds {
            unix_seconds: 1_000_000,
        };
        let last = InstantSeconds {
            unix_seconds: 999_000,
        };
        let next = next_fire_after(&schedule, now, Some(last), 0).expect("next");
        assert_eq!(next.unix_seconds, 1_002_600);
    }

    #[test]
    fn time_of_day_picks_later_today() {
        let profile = ProfileId::new();
        let mut schedule = Schedule::manual("ToD", profile);
        schedule.rule = ScheduleRule::TimeOfDay {
            times: vec![
                LocalTimeOfDay::new(9, 0).unwrap(),
                LocalTimeOfDay::new(18, 0).unwrap(),
            ],
        };
        // 2024-01-01 10:00 UTC
        let now = InstantSeconds {
            unix_seconds: 1_704_103_200,
        };
        let next = next_fire_after(&schedule, now, None, 0).expect("next");
        let local = next.to_local(0);
        assert_eq!(
            local.time,
            LocalTimeOfDay {
                hour: 18,
                minute: 0
            }
        );
    }

    #[test]
    fn time_of_day_skips_past_minute_when_seconds_elapsed() {
        let profile = ProfileId::new();
        let mut schedule = Schedule::manual("ToD", profile);
        schedule.rule = ScheduleRule::TimeOfDay {
            times: vec![LocalTimeOfDay::new(18, 0).unwrap()],
        };
        // 2024-01-01 18:00:30 UTC — the 18:00:00 slot is already past.
        let now = InstantSeconds {
            unix_seconds: 1_704_132_030,
        };
        let next = next_fire_after(&schedule, now, None, 0).expect("next");
        assert!(
            next.unix_seconds > now.unix_seconds,
            "next fire must be strictly after now when seconds have elapsed past the slot"
        );
        let local = next.to_local(0);
        assert_eq!(local.day, 2);
        assert_eq!(local.time.hour, 18);
        assert_eq!(local.time.minute, 0);
    }

    #[test]
    fn calendar_skips_disabled_weekdays() {
        let profile = ProfileId::new();
        let mut schedule = Schedule::manual("Weekdays", profile);
        // Monday only
        schedule.rule = ScheduleRule::Calendar {
            weekdays: 0b0000_0001,
            time: LocalTimeOfDay::new(8, 30).unwrap(),
        };
        // 2024-01-01 was Monday 00:00 UTC
        let now = InstantSeconds {
            unix_seconds: 1_704_067_200,
        };
        let next = next_fire_after(&schedule, now, None, 0).expect("next");
        let local = next.to_local(0);
        assert_eq!(local.weekday, 0);
        assert_eq!(local.time.hour, 8);
        assert_eq!(local.time.minute, 30);
    }

    #[test]
    fn rejects_short_interval() {
        let mut schedule = Schedule::interval("Too fast", ProfileId::new(), 30);
        assert_eq!(
            schedule.validate(),
            Err(ScheduleError::IntervalTooShort(30))
        );
        schedule.rule = ScheduleRule::Interval { every_seconds: 60 };
        assert_eq!(schedule.validate(), Ok(()));
    }

    #[test]
    fn civil_round_trip() {
        let instant = InstantSeconds {
            unix_seconds: 1_704_103_200,
        };
        let local = instant.to_local(0);
        assert_eq!(local.year, 2024);
        assert_eq!(local.month, 1);
        assert_eq!(local.day, 1);
        let back = instant_at_local(local.year, local.month, local.day, local.time, 0);
        assert_eq!(back.unix_seconds, instant.unix_seconds);
    }
}
