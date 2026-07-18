// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wallpaper automation schedules.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

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

/// Minutes past local midnight (0..=1439).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct MinutesOfDay(u16);

impl MinutesOfDay {
    /// Creates a validated local time-of-day.
    pub fn new(minutes: u16) -> Result<Self, ScheduleValidationError> {
        if minutes >= 24 * 60 {
            return Err(ScheduleValidationError::InvalidMinutesOfDay(minutes));
        }
        Ok(Self(minutes))
    }

    /// Returns minutes past local midnight.
    #[must_use]
    pub fn as_u16(self) -> u16 {
        self.0
    }

    /// Hours component (0..=23).
    #[must_use]
    pub fn hour(self) -> u16 {
        self.0 / 60
    }

    /// Minutes component (0..=59).
    #[must_use]
    pub fn minute(self) -> u16 {
        self.0 % 60
    }
}

/// Inclusive local time window within one civil day.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Inclusive start.
    pub start: MinutesOfDay,
    /// Inclusive end; may be less than start for overnight windows.
    pub end: MinutesOfDay,
}

impl TimeWindow {
    /// Creates a time window after validating both endpoints.
    pub fn new(start: MinutesOfDay, end: MinutesOfDay) -> Self {
        Self { start, end }
    }

    /// Returns whether `minutes` falls inside this window.
    #[must_use]
    pub fn contains(self, minutes: MinutesOfDay) -> bool {
        let start = self.start.as_u16();
        let end = self.end.as_u16();
        let value = minutes.as_u16();
        if start <= end {
            (start..=end).contains(&value)
        } else {
            value >= start || value <= end
        }
    }
}

/// Bit flags for days of the week (Monday = bit 0 … Sunday = bit 6).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WeekdayMask(u8);

impl WeekdayMask {
    /// Every day of the week.
    pub const ALL: Self = Self(0b0111_1111);

    /// Creates a mask from the low seven bits.
    #[must_use]
    pub fn new(bits: u8) -> Self {
        Self(bits & 0b0111_1111)
    }

    /// Returns whether the ISO weekday (1 = Monday … 7 = Sunday) is enabled.
    #[must_use]
    pub fn contains_iso_weekday(self, weekday: u8) -> bool {
        if !(1..=7).contains(&weekday) {
            return false;
        }
        let bit = weekday - 1;
        (self.0 & (1 << bit)) != 0
    }

    /// Raw bit mask.
    #[must_use]
    pub fn bits(self) -> u8 {
        self.0
    }
}

/// One calendar-like rule: selected weekdays plus a local time window.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CalendarRule {
    /// Days when this rule is active.
    pub weekdays: WeekdayMask,
    /// Local time window on those days.
    pub window: TimeWindow,
}

/// Geographic coordinates used for solar schedules.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeoLocation {
    /// Latitude in decimal degrees (−90..=90).
    pub latitude_deg: f64,
    /// Longitude in decimal degrees (−180..=180).
    pub longitude_deg: f64,
}

impl GeoLocation {
    /// Validates finite coordinates inside geographic bounds.
    pub fn validate(self) -> Result<(), ScheduleValidationError> {
        if !self.latitude_deg.is_finite() || !(-90.0..=90.0).contains(&self.latitude_deg) {
            return Err(ScheduleValidationError::InvalidLatitude);
        }
        if !self.longitude_deg.is_finite() || !(-180.0..=180.0).contains(&self.longitude_deg) {
            return Err(ScheduleValidationError::InvalidLongitude);
        }
        Ok(())
    }
}

/// How a schedule decides when to rotate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleKind {
    /// Rotate every N seconds while enabled and not paused.
    Interval {
        /// Seconds between rotations; must be at least one.
        seconds: u64,
    },
    /// Rotate while the local clock is inside any listed window.
    TimeOfDay {
        /// Local windows when rotation is allowed / triggered.
        windows: Vec<TimeWindow>,
    },
    /// Rotate at sunrise and sunset for a fixed location.
    SunriseSunset {
        /// Observer location.
        location: GeoLocation,
        /// Minutes to shift sunrise (negative = earlier).
        sunrise_offset_minutes: i32,
        /// Minutes to shift sunset (negative = earlier).
        sunset_offset_minutes: i32,
    },
    /// Calendar-like weekday + window rules.
    Calendar {
        /// Independent weekday windows.
        rules: Vec<CalendarRule>,
    },
}

/// Versioned automation schedule bound to a profile and rotation queue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    /// Serialized schema version.
    pub schema_version: u16,
    /// Stable identity.
    pub id: ScheduleId,
    /// User-visible name.
    pub name: String,
    /// Whether the scheduler may fire this schedule.
    pub enabled: bool,
    /// Fixed UTC offset for local civil time (minutes east of UTC).
    pub utc_offset_minutes: i32,
    /// Trigger policy.
    pub kind: ScheduleKind,
}

impl Schedule {
    /// Creates an enabled interval schedule with a one-hour default.
    #[must_use]
    pub fn interval(name: impl Into<String>, seconds: u64) -> Self {
        Self {
            schema_version: SCHEDULE_SCHEMA_VERSION,
            id: ScheduleId::new(),
            name: name.into(),
            enabled: true,
            utc_offset_minutes: 0,
            kind: ScheduleKind::Interval { seconds },
        }
    }

    /// Validates schema, name, offset, and kind-specific invariants.
    pub fn validate(&self) -> Result<(), ScheduleValidationError> {
        if self.schema_version != SCHEDULE_SCHEMA_VERSION {
            return Err(ScheduleValidationError::UnsupportedSchema(
                self.schema_version,
            ));
        }
        if self.name.trim().is_empty() {
            return Err(ScheduleValidationError::EmptyName);
        }
        if !(-14 * 60..=14 * 60).contains(&self.utc_offset_minutes) {
            return Err(ScheduleValidationError::InvalidUtcOffset);
        }
        match &self.kind {
            ScheduleKind::Interval { seconds } => {
                if *seconds == 0 {
                    return Err(ScheduleValidationError::ZeroInterval);
                }
            }
            ScheduleKind::TimeOfDay { windows } => {
                if windows.is_empty() {
                    return Err(ScheduleValidationError::EmptyWindows);
                }
            }
            ScheduleKind::SunriseSunset { location, .. } => location.validate()?,
            ScheduleKind::Calendar { rules } => {
                if rules.is_empty() {
                    return Err(ScheduleValidationError::EmptyCalendarRules);
                }
                for rule in rules {
                    if rule.weekdays.bits() == 0 {
                        return Err(ScheduleValidationError::EmptyWeekdayMask);
                    }
                }
            }
        }
        Ok(())
    }
}

/// Invalid schedule model.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ScheduleValidationError {
    /// Schema version is not supported by this build.
    #[error("unsupported schedule schema version: {0}")]
    UnsupportedSchema(u16),
    /// Schedule names must contain visible characters.
    #[error("schedule name cannot be empty")]
    EmptyName,
    /// UTC offsets must stay within ±14 hours.
    #[error("utc offset minutes must be between -840 and 840")]
    InvalidUtcOffset,
    /// Interval schedules require a positive period.
    #[error("interval schedule seconds must be greater than zero")]
    ZeroInterval,
    /// Time-of-day schedules need at least one window.
    #[error("time-of-day schedule requires at least one window")]
    EmptyWindows,
    /// Calendar schedules need at least one rule.
    #[error("calendar schedule requires at least one rule")]
    EmptyCalendarRules,
    /// A calendar rule must select at least one weekday.
    #[error("calendar rule weekday mask cannot be empty")]
    EmptyWeekdayMask,
    /// Minutes-of-day must be less than 1440.
    #[error("minutes of day out of range: {0}")]
    InvalidMinutesOfDay(u16),
    /// Latitude must be finite and within ±90°.
    #[error("latitude must be a finite value between -90 and 90")]
    InvalidLatitude,
    /// Longitude must be finite and within ±180°.
    #[error("longitude must be a finite value between -180 and 180")]
    InvalidLongitude,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overnight_window_contains_midnight() {
        let window = TimeWindow::new(
            MinutesOfDay::new(22 * 60).unwrap(),
            MinutesOfDay::new(6 * 60).unwrap(),
        );
        assert!(window.contains(MinutesOfDay::new(23 * 60).unwrap()));
        assert!(window.contains(MinutesOfDay::new(0).unwrap()));
        assert!(window.contains(MinutesOfDay::new(5 * 60).unwrap()));
        assert!(!window.contains(MinutesOfDay::new(12 * 60).unwrap()));
    }

    #[test]
    fn interval_requires_positive_seconds() {
        let schedule = Schedule::interval("Hourly", 0);
        assert_eq!(
            schedule.validate(),
            Err(ScheduleValidationError::ZeroInterval)
        );
    }

    #[test]
    fn weekday_mask_monday() {
        let mask = WeekdayMask::new(0b0000_0001);
        assert!(mask.contains_iso_weekday(1));
        assert!(!mask.contains_iso_weekday(2));
    }
}
