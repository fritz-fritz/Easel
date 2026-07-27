// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared logical media clock for live wallpaper sessions.
//!
//! One [`PlaybackClock`] drives every display surface in a live group so crops
//! stay synchronized. Callers supply wall-clock deltas; the clock never reads
//! the system clock itself (deterministic tests and host adapters).

use thiserror::Error;

use crate::profile::{LoopMode, PlaybackPolicy};

/// Largest integer exactly representable in `f64` (2⁵³ − 1).
const MAX_EXACT_MS: u64 = (1_u64 << 53) - 1;

/// One logical media clock shared by all live display surfaces in a session.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackClock {
    /// Current media timeline position in milliseconds.
    position_ms: f64,
    /// Known media duration when the container reports one.
    duration_ms: Option<u64>,
    /// Playback speed multiplier (`PlaybackPolicy::rate`).
    rate: f64,
    /// End-of-stream behavior.
    loop_mode: LoopMode,
    /// Optional presentation frame-rate ceiling.
    max_fps: Option<u16>,
    /// Whether media time is frozen.
    paused: bool,
    /// True after `Once` reaches the end of a known duration.
    ended: bool,
    /// Wall milliseconds accumulated since the last presentable sample.
    since_present_ms: f64,
}

/// Sample produced by advancing the shared clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationSample {
    /// Media timeline position after the tick (milliseconds).
    pub media_time_ms: u64,
    /// Whether surfaces should refresh from this sample (FPS ceiling gate).
    pub should_present: bool,
    /// True when `Once` playback has finished.
    pub ended: bool,
}

impl PlaybackClock {
    /// Builds a clock from a validated playback policy and optional duration.
    pub fn from_policy(
        policy: &PlaybackPolicy,
        duration_ms: Option<u64>,
    ) -> Result<Self, PlaybackClockError> {
        if !policy.rate.is_finite() || policy.rate <= 0.0 {
            return Err(PlaybackClockError::InvalidRate);
        }
        if policy.maximum_frames_per_second == Some(0) {
            return Err(PlaybackClockError::InvalidFrameRateLimit);
        }
        Ok(Self {
            position_ms: 0.0,
            duration_ms,
            rate: policy.rate,
            loop_mode: policy.loop_mode,
            max_fps: policy.maximum_frames_per_second,
            paused: false,
            ended: false,
            since_present_ms: 0.0,
        })
    }

    /// Current media position in whole milliseconds.
    #[must_use]
    pub fn position_ms(&self) -> u64 {
        let clamped = self.position_ms.max(0.0).floor();
        let max_exact = max_exact_ms_f64();
        if clamped >= max_exact {
            MAX_EXACT_MS
        } else {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                clamped as u64
            }
        }
    }

    /// Known media duration, when reported.
    #[must_use]
    pub const fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }

    /// Whether the clock is paused.
    #[must_use]
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    /// Whether `Once` playback has finished.
    #[must_use]
    pub const fn is_ended(&self) -> bool {
        self.ended
    }

    /// Freezes media time without discarding the current position.
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resumes advancing media time from the shared position.
    pub fn resume(&mut self) {
        if self.ended {
            return;
        }
        self.paused = false;
    }

    /// Seeks to an absolute media position (clamped to the known duration).
    pub fn seek_ms(&mut self, position_ms: u64) {
        let was_ended = self.ended;
        let clamped = match self.duration_ms {
            Some(duration) if duration > 0 => position_ms.min(duration),
            _ => position_ms,
        };
        self.position_ms = u64_to_f64(clamped);
        if self
            .duration_ms
            .is_some_and(|duration| duration > 0 && clamped >= duration)
        {
            match self.loop_mode {
                LoopMode::Once => {
                    self.ended = true;
                    self.paused = true;
                }
                LoopMode::Loop => {
                    self.position_ms = 0.0;
                    self.ended = false;
                }
            }
        } else {
            self.ended = false;
            // Seeking away from Once EOF must not leave the clock stuck paused.
            if was_ended {
                self.paused = false;
            }
        }
        self.since_present_ms = 0.0;
    }

    /// Advances the clock by a wall-clock delta and returns the presentation sample.
    ///
    /// All displays in a live group must consume this same sample so crops share
    /// one media timeline (no per-monitor player drift).
    pub fn tick(&mut self, wall_delta_ms: u64) -> PresentationSample {
        let wall = u64_to_f64(wall_delta_ms);
        if self.paused || self.ended || wall <= 0.0 {
            return PresentationSample {
                media_time_ms: self.position_ms(),
                should_present: false,
                ended: self.ended,
            };
        }

        let was_ended = self.ended;
        self.position_ms += wall * self.rate;
        self.apply_end_of_stream();
        let reached_end = !was_ended && self.ended;

        self.since_present_ms += wall;
        // Once EOF must always present the final frame even if the FPS gate
        // would otherwise skip; later ticks stay paused/ended.
        let should_present = if reached_end {
            self.since_present_ms = 0.0;
            true
        } else {
            self.take_present_gate()
        };
        PresentationSample {
            media_time_ms: self.position_ms(),
            should_present,
            ended: self.ended,
        }
    }

    fn apply_end_of_stream(&mut self) {
        let Some(duration) = self.duration_ms.filter(|value| *value > 0) else {
            return;
        };
        let duration_f = u64_to_f64(duration);
        match self.loop_mode {
            LoopMode::Loop => {
                while self.position_ms >= duration_f {
                    self.position_ms -= duration_f;
                }
            }
            LoopMode::Once => {
                if self.position_ms >= duration_f {
                    self.position_ms = duration_f;
                    self.ended = true;
                    self.paused = true;
                }
            }
        }
    }

    fn take_present_gate(&mut self) -> bool {
        let Some(max_fps) = self.max_fps else {
            self.since_present_ms = 0.0;
            return true;
        };
        let min_period_ms = 1000.0 / f64::from(max_fps);
        if self.since_present_ms + f64::EPSILON >= min_period_ms {
            self.since_present_ms %= min_period_ms;
            true
        } else {
            false
        }
    }
}

fn max_exact_ms_f64() -> f64 {
    #[allow(clippy::cast_precision_loss)]
    {
        MAX_EXACT_MS as f64
    }
}

fn u64_to_f64(value: u64) -> f64 {
    // Keep conversions inside the exact integer range of f64 (2⁵³ − 1).
    let clamped = value.min(MAX_EXACT_MS);
    #[allow(clippy::cast_precision_loss)]
    {
        clamped as f64
    }
}

/// Invalid playback clock configuration.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PlaybackClockError {
    /// Rate must be finite and greater than zero.
    #[error("playback rate must be a finite value greater than zero")]
    InvalidRate,
    /// Frame-rate ceiling cannot be zero.
    #[error("playback frame-rate limit must be greater than zero")]
    InvalidFrameRateLimit,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(rate: f64, loop_mode: LoopMode, max_fps: Option<u16>) -> PlaybackPolicy {
        PlaybackPolicy {
            loop_mode,
            rate,
            maximum_frames_per_second: max_fps,
            pause_on_battery: true,
            pause_for_full_screen_app: true,
        }
    }

    #[test]
    fn rate_two_advances_media_twice_wall() {
        let mut clock =
            PlaybackClock::from_policy(&policy(2.0, LoopMode::Loop, None), Some(10_000)).unwrap();
        let sample = clock.tick(100);
        assert_eq!(sample.media_time_ms, 200);
        assert!(sample.should_present);
        assert!(!sample.ended);
    }

    #[test]
    fn max_fps_gates_presentation() {
        let mut clock =
            PlaybackClock::from_policy(&policy(1.0, LoopMode::Loop, Some(30)), Some(10_000))
                .unwrap();
        // 20 ms wall < 33.3 ms period → media advances but no present.
        let first = clock.tick(20);
        assert_eq!(first.media_time_ms, 20);
        assert!(!first.should_present);
        // Another 20 ms crosses the ~33 ms gate.
        let second = clock.tick(20);
        assert_eq!(second.media_time_ms, 40);
        assert!(second.should_present);
    }

    #[test]
    fn once_ends_and_clamps_at_duration() {
        let mut clock =
            PlaybackClock::from_policy(&policy(1.0, LoopMode::Once, None), Some(100)).unwrap();
        let sample = clock.tick(150);
        assert_eq!(sample.media_time_ms, 100);
        assert!(sample.ended);
        assert!(sample.should_present);
        assert!(clock.is_paused());
        let after = clock.tick(50);
        assert_eq!(after.media_time_ms, 100);
        assert!(!after.should_present);
        assert!(after.ended);
    }

    #[test]
    fn once_end_presents_even_when_fps_gate_would_skip() {
        let mut clock =
            PlaybackClock::from_policy(&policy(1.0, LoopMode::Once, Some(30)), Some(50)).unwrap();
        let mid = clock.tick(20);
        assert_eq!(mid.media_time_ms, 20);
        assert!(!mid.should_present);
        assert!(!mid.ended);
        let end = clock.tick(40);
        assert_eq!(end.media_time_ms, 50);
        assert!(end.ended);
        assert!(
            end.should_present,
            "final Once frame must present despite FPS gate"
        );
    }

    #[test]
    fn seek_after_once_end_resumes_playback() {
        let mut clock =
            PlaybackClock::from_policy(&policy(1.0, LoopMode::Once, None), Some(100)).unwrap();
        assert!(clock.tick(150).ended);
        clock.seek_ms(10);
        assert!(!clock.is_ended());
        assert!(!clock.is_paused());
        assert_eq!(clock.tick(5).media_time_ms, 15);
    }

    #[test]
    fn loop_wraps_at_duration() {
        let mut clock =
            PlaybackClock::from_policy(&policy(1.0, LoopMode::Loop, None), Some(100)).unwrap();
        let sample = clock.tick(250);
        assert_eq!(sample.media_time_ms, 50);
        assert!(!sample.ended);
    }

    #[test]
    fn pause_freezes_media_time() {
        let mut clock =
            PlaybackClock::from_policy(&policy(1.0, LoopMode::Loop, None), Some(10_000)).unwrap();
        assert_eq!(clock.tick(40).media_time_ms, 40);
        clock.pause();
        assert_eq!(clock.tick(100).media_time_ms, 40);
        clock.resume();
        assert_eq!(clock.tick(10).media_time_ms, 50);
    }

    #[test]
    fn multi_display_consumers_share_one_timeline() {
        // Drift is zero by construction: one clock, many consumers.
        let mut clock =
            PlaybackClock::from_policy(&policy(1.0, LoopMode::Loop, Some(30)), Some(10_000))
                .unwrap();
        let mut left = 0;
        let mut right = 0;
        for _ in 0..5 {
            let sample = clock.tick(40);
            left = sample.media_time_ms;
            right = sample.media_time_ms;
        }
        assert_eq!(left, right);
        assert_eq!(left, 200);
    }

    #[test]
    fn invalid_rate_is_rejected() {
        assert_eq!(
            PlaybackClock::from_policy(&policy(0.0, LoopMode::Loop, None), None),
            Err(PlaybackClockError::InvalidRate)
        );
    }

    #[test]
    fn zero_frame_rate_limit_is_rejected() {
        assert_eq!(
            PlaybackClock::from_policy(&policy(1.0, LoopMode::Loop, Some(0)), None),
            Err(PlaybackClockError::InvalidFrameRateLimit)
        );
    }
}
