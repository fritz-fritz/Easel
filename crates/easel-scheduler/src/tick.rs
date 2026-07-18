// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Schedule due-checking and high-level automation ticks.

use easel_core::{AutomationState, MinutesOfDay, RotationQueue, Schedule, ScheduleKind};

use crate::select::{SelectionError, SelectionOutcome, select_next_asset};
use crate::solar::solar_events_for_day;

/// Result of asking whether a schedule wants to rotate now.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleEvaluation {
    /// Whether a rotation should fire at `now_unix`.
    pub due: bool,
    /// Explainable reason.
    pub reason: String,
    /// Next interesting unix timestamp when known (interval / solar).
    pub next_unix: Option<u64>,
}

/// Outcome of one scheduler tick.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TickDecision {
    /// Automation is paused by the user.
    Paused {
        /// Explainable reason.
        reason: String,
    },
    /// Schedule is disabled or not due.
    Idle {
        /// Explainable reason.
        reason: String,
        /// Optional next due time.
        next_unix: Option<u64>,
    },
    /// A new asset should be applied.
    Apply {
        /// Selection details.
        selection: SelectionOutcome,
        /// Schedule evaluation that triggered the apply.
        schedule: ScheduleEvaluation,
    },
    /// Selection failed.
    Failed {
        /// Explainable reason.
        reason: String,
    },
}

/// Evaluates whether `schedule` is due at `now_unix`.
#[must_use]
pub fn evaluate_schedule(
    schedule: &Schedule,
    state: &AutomationState,
    now_unix: u64,
) -> ScheduleEvaluation {
    if !schedule.enabled {
        return ScheduleEvaluation {
            due: false,
            reason: "schedule is disabled".to_owned(),
            next_unix: None,
        };
    }

    match &schedule.kind {
        ScheduleKind::Interval { seconds } => {
            let last = state.last_applied_unix.unwrap_or(0);
            let next = last.saturating_add(*seconds);
            if state.last_applied_unix.is_none() || now_unix >= next {
                ScheduleEvaluation {
                    due: true,
                    reason: format!("interval of {seconds}s elapsed"),
                    next_unix: Some(now_unix.saturating_add(*seconds)),
                }
            } else {
                ScheduleEvaluation {
                    due: false,
                    reason: format!("next interval apply at unix {next}"),
                    next_unix: Some(next),
                }
            }
        }
        ScheduleKind::TimeOfDay { windows } => {
            let local = local_minutes(now_unix, schedule.utc_offset_minutes);
            let active = windows.iter().any(|window| window.contains(local));
            ScheduleEvaluation {
                due: active && should_fire_window(state, now_unix),
                reason: if active {
                    format!(
                        "local time {:02}:{:02} is inside a configured window",
                        local.hour(),
                        local.minute()
                    )
                } else {
                    format!(
                        "local time {:02}:{:02} is outside configured windows",
                        local.hour(),
                        local.minute()
                    )
                },
                next_unix: None,
            }
        }
        ScheduleKind::SunriseSunset {
            location,
            sunrise_offset_minutes,
            sunset_offset_minutes,
        } => {
            let events = solar_events_for_day(*location, now_unix);
            let sunrise = apply_offset(events.sunrise_unix, *sunrise_offset_minutes);
            let sunset = apply_offset(events.sunset_unix, *sunset_offset_minutes);
            let due = within_minute(now_unix, sunrise) || within_minute(now_unix, sunset);
            let next = if now_unix < sunrise {
                Some(sunrise)
            } else if now_unix < sunset {
                Some(sunset)
            } else {
                let tomorrow = solar_events_for_day(*location, now_unix.saturating_add(86_400));
                Some(apply_offset(tomorrow.sunrise_unix, *sunrise_offset_minutes))
            };
            ScheduleEvaluation {
                due: due && should_fire_window(state, now_unix),
                reason: if due {
                    "local solar event (sunrise/sunset) is due".to_owned()
                } else {
                    format!("waiting for next solar event at unix {next:?}")
                },
                next_unix: next,
            }
        }
        ScheduleKind::Calendar { rules } => {
            let weekday = iso_weekday(now_unix, schedule.utc_offset_minutes);
            let local = local_minutes(now_unix, schedule.utc_offset_minutes);
            let active = rules.iter().any(|rule| {
                rule.weekdays.contains_iso_weekday(weekday) && rule.window.contains(local)
            });
            ScheduleEvaluation {
                due: active && should_fire_window(state, now_unix),
                reason: if active {
                    format!(
                        "calendar rule matches weekday {weekday} at {:02}:{:02}",
                        local.hour(),
                        local.minute()
                    )
                } else {
                    format!(
                        "no calendar rule matches weekday {weekday} at {:02}:{:02}",
                        local.hour(),
                        local.minute()
                    )
                },
                next_unix: None,
            }
        }
    }
}

/// Runs one scheduler tick against the active schedule and queue.
#[must_use]
pub fn tick(
    state: &AutomationState,
    schedule: Option<&Schedule>,
    queue: Option<&RotationQueue>,
    now_unix: u64,
    force: bool,
) -> TickDecision {
    if state.paused && !force {
        return TickDecision::Paused {
            reason: "automation is paused".to_owned(),
        };
    }

    let Some(queue) = queue else {
        return TickDecision::Failed {
            reason: "no active rotation queue".to_owned(),
        };
    };

    if force {
        return match select_next_asset(queue, &state.recent_asset_ids, now_unix) {
            Ok(selection) => TickDecision::Apply {
                selection,
                schedule: ScheduleEvaluation {
                    due: true,
                    reason: "forced skip/apply-next".to_owned(),
                    next_unix: None,
                },
            },
            Err(error) => TickDecision::Failed {
                reason: error.to_string(),
            },
        };
    }

    let Some(schedule) = schedule else {
        return TickDecision::Idle {
            reason: "no active schedule".to_owned(),
            next_unix: None,
        };
    };

    let evaluation = evaluate_schedule(schedule, state, now_unix);
    if !evaluation.due {
        return TickDecision::Idle {
            reason: evaluation.reason.clone(),
            next_unix: evaluation.next_unix,
        };
    }

    match select_next_asset(queue, &state.recent_asset_ids, now_unix) {
        Ok(selection) => TickDecision::Apply {
            selection,
            schedule: evaluation,
        },
        Err(SelectionError::EmptyQueue) => TickDecision::Failed {
            reason: "rotation queue is empty".to_owned(),
        },
        Err(error) => TickDecision::Failed {
            reason: error.to_string(),
        },
    }
}

fn should_fire_window(state: &AutomationState, now_unix: u64) -> bool {
    // Window schedules fire at most once per local minute.
    match state.last_applied_unix {
        None => true,
        Some(last) => now_unix.saturating_sub(last) >= 60,
    }
}

fn within_minute(now_unix: u64, event_unix: u64) -> bool {
    now_unix >= event_unix && now_unix < event_unix.saturating_add(60)
}

fn apply_offset(unix: u64, offset_minutes: i32) -> u64 {
    apply_seconds_offset(unix, i64::from(offset_minutes).saturating_mul(60))
}

fn apply_seconds_offset(unix: u64, offset_seconds: i64) -> u64 {
    if let Ok(positive) = u64::try_from(offset_seconds) {
        unix.saturating_add(positive)
    } else {
        unix.saturating_sub(offset_seconds.unsigned_abs())
    }
}

fn local_minutes(now_unix: u64, utc_offset_minutes: i32) -> MinutesOfDay {
    let local = apply_seconds_offset(now_unix, i64::from(utc_offset_minutes).saturating_mul(60));
    let minutes = u16::try_from((local / 60) % (24 * 60)).unwrap_or(0);
    MinutesOfDay::new(minutes).unwrap_or_else(|_| MinutesOfDay::new(0).expect("0 is valid"))
}

fn iso_weekday(now_unix: u64, utc_offset_minutes: i32) -> u8 {
    let local = apply_seconds_offset(now_unix, i64::from(utc_offset_minutes).saturating_mul(60));
    // 1970-01-01 was a Thursday (ISO 4).
    let days = local / 86_400;
    u8::try_from(((days + 3) % 7) + 1).unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use easel_core::{AssetId, AutomationState, RotationQueue, Schedule};

    #[test]
    fn interval_due_when_never_applied() {
        let schedule = Schedule::interval("Hourly", 3600);
        let state = AutomationState::idle();
        let evaluation = evaluate_schedule(&schedule, &state, 100);
        assert!(evaluation.due);
    }

    #[test]
    fn pause_blocks_tick() {
        let mut state = AutomationState::idle();
        state.paused = true;
        let mut queue = RotationQueue::new("Desk");
        queue.assets = vec![AssetId::new()];
        let decision = tick(&state, None, Some(&queue), 10, false);
        assert!(matches!(decision, TickDecision::Paused { .. }));
    }

    #[test]
    fn force_skip_ignores_pause() {
        let mut state = AutomationState::idle();
        state.paused = true;
        let asset = AssetId::new();
        let mut queue = RotationQueue::new("Desk");
        queue.assets = vec![asset];
        let decision = tick(&state, None, Some(&queue), 10, true);
        match decision {
            TickDecision::Apply { selection, .. } => assert_eq!(selection.asset_id, asset),
            other => panic!("unexpected {other:?}"),
        }
    }
}
