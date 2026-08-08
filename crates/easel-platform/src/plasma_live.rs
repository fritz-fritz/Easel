// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Plasma live-wallpaper host via the Easel `Plasma/Wallpaper` plugin (ADR 0008).
//!
//! Desktop owns the shared [`PlaybackClock`] and pause policy. The plugin renders
//! muted `AnimatedImage` / `MediaPlayer` crops from `active.json` IPC. All
//! containments consume the same `media_time_ms` so multi-display crops stay
//! synchronized (no independent per-monitor players).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use easel_core::{PlaybackClock, PlaybackPolicy};

use crate::live_policy::{LivePolicySensors, pause_reason_for, probe_live_policy_sensors};
use crate::plasma::{easel_plasma_plugin_id, ensure_easel_plugin_bound};
use crate::plasma_state::{
    PlasmaLiveClockSnapshot, PlasmaWallpaperState, publish_plasma_live_state,
};
use crate::{
    BackendError, DisplayWallpaper, LiveBackendCapabilities, LiveDisplaySurface,
    LiveWallpaperBackend, LiveWallpaperOutput, LiveWallpaperSession,
};

/// Tick interval for shared-clock publish + policy sensors (~30 Hz ceiling).
const LIVE_SESSION_TICK: Duration = Duration::from_millis(33);

/// Plasma live host that drives the Easel wallpaper plugin.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlasmaLiveBackend;

impl LiveWallpaperBackend for PlasmaLiveBackend {
    fn id(&self) -> &'static str {
        "plasma6-live"
    }

    fn capabilities(&self) -> LiveBackendCapabilities {
        LiveBackendCapabilities {
            animated_images: true,
            video: true,
            per_display_surfaces: true,
            shared_media_clock: true,
            hardware_decode: false,
            pause_when_occluded: false,
        }
    }

    fn start(
        &self,
        output: &LiveWallpaperOutput,
        policy: PlaybackPolicy,
    ) -> Result<Box<dyn LiveWallpaperSession>, BackendError> {
        if easel_plasma_plugin_id().is_none() {
            return Err(BackendError::LiveWallpaperUnsupported);
        }
        let LiveWallpaperOutput::PerDisplay(surfaces) = output else {
            return Err(BackendError::UnsupportedOutput);
        };
        if surfaces.is_empty() {
            return Err(BackendError::Platform(
                "live wallpaper requires at least one display surface".into(),
            ));
        }
        for surface in surfaces {
            validate_media_paths(&surface.media.source, &surface.media.poster_frame)?;
        }

        let media_kind = media_kind_for_path(&surfaces[0].media.source);
        let source_width = surfaces[0].source_width;
        let source_height = surfaces[0].source_height;
        if source_width == 0 || source_height == 0 {
            return Err(BackendError::Platform(
                "live wallpaper source dimensions must be non-zero".into(),
            ));
        }

        let clock = PlaybackClock::from_policy(&policy, None)
            .map_err(|error| BackendError::Platform(format!("invalid playback policy: {error}")))?;

        let session = PlasmaLiveSession::start(
            surfaces,
            source_width,
            source_height,
            media_kind,
            policy,
            clock,
        )?;
        Ok(Box::new(session))
    }
}

fn validate_media_paths(source: &Path, poster: &Path) -> Result<(), BackendError> {
    if !source.is_file() {
        return Err(BackendError::MissingOutput(source.to_path_buf()));
    }
    if !poster.is_file() {
        return Err(BackendError::MissingOutput(poster.to_path_buf()));
    }
    Ok(())
}

fn media_kind_for_path(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "gif" | "webp" => "animated_image",
        _ => "video",
    }
}

fn posters_as_wallpapers(surfaces: &[LiveDisplaySurface]) -> Vec<DisplayWallpaper> {
    surfaces
        .iter()
        .map(|surface| DisplayWallpaper {
            display_id: surface.display_id,
            path: surface.media.poster_frame.clone(),
            logical_rect: surface.logical_rect,
        })
        .collect()
}

struct PlasmaLiveSessionInner {
    surfaces: Vec<LiveDisplaySurface>,
    source_width: u32,
    source_height: u32,
    media_kind: String,
    policy: PlaybackPolicy,
    clock: PlaybackClock,
    pause_reason: String,
    /// Manual pause from [`LiveWallpaperSession::pause`] (distinct from policy).
    manual_pause: bool,
}

/// Running Plasma live session with background clock + policy ticks.
pub struct PlasmaLiveSession {
    inner: Arc<Mutex<PlasmaLiveSessionInner>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl PlasmaLiveSession {
    fn start(
        surfaces: &[LiveDisplaySurface],
        source_width: u32,
        source_height: u32,
        media_kind: &str,
        policy: PlaybackPolicy,
        clock: PlaybackClock,
    ) -> Result<Self, BackendError> {
        let inner = Arc::new(Mutex::new(PlasmaLiveSessionInner {
            surfaces: surfaces.to_vec(),
            source_width,
            source_height,
            media_kind: media_kind.to_owned(),
            policy,
            clock,
            pause_reason: String::new(),
            manual_pause: false,
        }));

        {
            let guard = inner.lock().map_err(|_| {
                BackendError::Platform("live session mutex poisoned during start".into())
            })?;
            publish_session_state(&guard)?;
            let wallpapers = posters_as_wallpapers(&guard.surfaces);
            let state_path = crate::plasma_state::default_plasma_wallpaper_state_path();
            ensure_easel_plugin_bound(&wallpapers, &state_path)?;
        }

        let stop = Arc::new(AtomicBool::new(false));
        let worker_inner = Arc::clone(&inner);
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("easel-plasma-live".into())
            .spawn(move || live_session_worker(&worker_inner, &worker_stop))
            .map_err(|error| {
                BackendError::Platform(format!("failed to start live session worker: {error}"))
            })?;

        Ok(Self {
            inner,
            stop,
            worker: Some(worker),
        })
    }
}

impl LiveWallpaperSession for PlasmaLiveSession {
    fn pause(&mut self) -> Result<(), BackendError> {
        let mut guard = self.inner.lock().map_err(|_| {
            BackendError::Platform("live session mutex poisoned during pause".into())
        })?;
        guard.manual_pause = true;
        guard.clock.pause();
        guard.pause_reason = "manual".into();
        publish_session_state(&guard)
    }

    fn resume(&mut self) -> Result<(), BackendError> {
        let mut guard = self.inner.lock().map_err(|_| {
            BackendError::Platform("live session mutex poisoned during resume".into())
        })?;
        guard.manual_pause = false;
        // Policy sensors may still require pause; worker reapplies on next tick.
        let sensors = probe_live_policy_sensors();
        if let Some(reason) = pause_reason_for(&guard.policy, &sensors) {
            guard.clock.pause();
            guard.pause_reason = reason.as_str().into();
        } else {
            guard.clock.resume();
            guard.pause_reason.clear();
        }
        publish_session_state(&guard)
    }

    fn stop(mut self: Box<Self>) -> Result<(), BackendError> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        // Leave poster stills active: rewrite state as still-only from posters.
        let guard = self.inner.lock().map_err(|_| {
            BackendError::Platform("live session mutex poisoned during stop".into())
        })?;
        let wallpapers = posters_as_wallpapers(&guard.surfaces);
        let state = PlasmaWallpaperState::from_wallpapers(&wallpapers);
        publish_plasma_live_state(&state)?;
        Ok(())
    }
}

impl Drop for PlasmaLiveSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn live_session_worker(inner: &Arc<Mutex<PlasmaLiveSessionInner>>, stop: &Arc<AtomicBool>) {
    let mut last = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        thread::sleep(LIVE_SESSION_TICK);
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let now = Instant::now();
        let wall_delta_ms =
            u64::try_from(now.saturating_duration_since(last).as_millis()).unwrap_or(u64::MAX);
        last = now;

        let Ok(mut guard) = inner.lock() else {
            break;
        };
        apply_policy_to_clock(&mut guard, probe_live_policy_sensors());
        if !guard.manual_pause {
            let _ = guard.clock.tick(wall_delta_ms);
        }
        let _ = publish_session_state(&guard);
    }
}

fn apply_policy_to_clock(inner: &mut PlasmaLiveSessionInner, sensors: LivePolicySensors) {
    if inner.manual_pause {
        inner.clock.pause();
        if inner.pause_reason.is_empty() {
            inner.pause_reason = "manual".into();
        }
        return;
    }
    if let Some(reason) = pause_reason_for(&inner.policy, &sensors) {
        inner.clock.pause();
        inner.pause_reason = reason.as_str().into();
    } else if inner.clock.is_paused() && !inner.clock.is_ended() {
        inner.clock.resume();
        inner.pause_reason.clear();
    } else if !inner.clock.is_paused() {
        inner.pause_reason.clear();
    }
}

fn publish_session_state(inner: &PlasmaLiveSessionInner) -> Result<(), BackendError> {
    let state = PlasmaWallpaperState::from_live_surfaces(
        &inner.surfaces,
        inner.source_width,
        inner.source_height,
        &inner.media_kind,
        &inner.policy,
        &PlasmaLiveClockSnapshot {
            paused: inner.clock.is_paused(),
            pause_reason: inner.pause_reason.clone(),
            media_time_ms: inner.clock.position_ms(),
            duration_ms: inner.clock.duration_ms(),
        },
    );
    publish_plasma_live_state(&state).map(|_| ())
}

/// Test helper: builds a live state document without starting Plasma D-Bus.
#[cfg(test)]
pub(crate) fn test_live_state_from_surfaces(
    surfaces: &[LiveDisplaySurface],
    policy: &PlaybackPolicy,
) -> PlasmaWallpaperState {
    PlasmaWallpaperState::from_live_surfaces(
        surfaces,
        surfaces.first().map_or(1, |s| s.source_width.max(1)),
        surfaces.first().map_or(1, |s| s.source_height.max(1)),
        "video",
        policy,
        &PlasmaLiveClockSnapshot {
            paused: false,
            pause_reason: String::new(),
            media_time_ms: 0,
            duration_ms: None,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveMediaOutput, SourceUvRect};
    use easel_core::{DisplayId, LogicalRect, LoopMode};

    fn surface() -> LiveDisplaySurface {
        let root = std::env::temp_dir().join(format!(
            "easel-plasma-live-sess-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let _ = std::fs::create_dir_all(&root);
        let source = root.join("a.gif");
        let poster = root.join("p.png");
        std::fs::write(&source, b"gif").unwrap();
        std::fs::write(&poster, b"png").unwrap();
        LiveDisplaySurface {
            display_id: DisplayId::from_u128(1),
            logical_rect: LogicalRect {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
            media: LiveMediaOutput {
                source,
                poster_frame: poster,
            },
            source_uv: SourceUvRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            source_width: 200,
            source_height: 100,
        }
    }

    #[test]
    fn media_kind_detects_gif() {
        assert_eq!(
            media_kind_for_path(Path::new("/tmp/x.GIF")),
            "animated_image"
        );
        assert_eq!(media_kind_for_path(Path::new("/tmp/x.mp4")), "video");
    }

    #[test]
    fn policy_pause_updates_clock() {
        let mut inner = PlasmaLiveSessionInner {
            surfaces: vec![surface()],
            source_width: 200,
            source_height: 100,
            media_kind: "animated_image".into(),
            policy: PlaybackPolicy {
                loop_mode: LoopMode::Loop,
                rate: 1.0,
                maximum_frames_per_second: Some(30),
                pause_on_battery: true,
                pause_for_full_screen_app: true,
            },
            clock: PlaybackClock::from_policy(&PlaybackPolicy::default(), None).unwrap(),
            pause_reason: String::new(),
            manual_pause: false,
        };
        apply_policy_to_clock(
            &mut inner,
            LivePolicySensors {
                on_battery: true,
                ..LivePolicySensors::default()
            },
        );
        assert!(inner.clock.is_paused());
        assert_eq!(inner.pause_reason, "battery");
        apply_policy_to_clock(&mut inner, LivePolicySensors::default());
        assert!(!inner.clock.is_paused());
        assert!(inner.pause_reason.is_empty());
    }

    #[test]
    fn live_state_builder_marks_live_mode() {
        let policy = PlaybackPolicy::default();
        let state = test_live_state_from_surfaces(&[surface()], &policy);
        assert!(matches!(
            state.mode,
            crate::plasma_state::PlasmaWallpaperMode::Live
        ));
        assert!(state.live.is_some());
    }
}
