// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! App-owned desktop-surface live wallpaper host (ADR 0014).
//!
//! Publishes the same live IPC document shape as the Plasma plugin host into
//! `desktop-live/active.json`. The Easel desktop process renders muted
//! per-display Qt windows from that file. Unlike Plasma, surfaces die when the
//! desktop app exits — probe text and Compose must label this experimental.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use easel_core::{PlaybackClock, PlaybackPolicy};

use crate::live_policy::{LivePolicySensors, pause_reason_for, probe_live_policy_sensors};
use crate::plasma_state::{
    PlasmaLiveClockSnapshot, PlasmaWallpaperState, publish_desktop_live_state,
};
use crate::{
    BackendError, DisplayWallpaper, LiveBackendCapabilities, LiveDisplaySurface,
    LiveWallpaperBackend, LiveWallpaperOutput, LiveWallpaperSession,
};

const LIVE_SESSION_TICK: Duration = Duration::from_millis(33);

/// Experimental live host that drives app-owned desktop surfaces via IPC.
#[derive(Clone, Copy, Debug, Default)]
pub struct DesktopSurfaceLiveBackend;

impl LiveWallpaperBackend for DesktopSurfaceLiveBackend {
    fn id(&self) -> &'static str {
        "desktop-surface-live"
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
        validate_uniform_live_surfaces(surfaces, source_width, source_height)?;

        let clock = PlaybackClock::from_policy(&policy, None)
            .map_err(|error| BackendError::Platform(format!("invalid playback policy: {error}")))?;

        let session = DesktopLiveSession::start(
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

fn validate_uniform_live_surfaces(
    surfaces: &[LiveDisplaySurface],
    source_width: u32,
    source_height: u32,
) -> Result<(), BackendError> {
    let Some(first) = surfaces.first() else {
        return Ok(());
    };
    let first_source = first.media.source.as_path();
    for surface in surfaces.iter().skip(1) {
        if surface.media.source != first_source {
            return Err(BackendError::Platform(
                "live wallpaper surfaces must share one media source".into(),
            ));
        }
        if surface.source_width != source_width || surface.source_height != source_height {
            return Err(BackendError::Platform(
                "live wallpaper surfaces must share oriented source dimensions".into(),
            ));
        }
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

struct DesktopLiveSessionInner {
    surfaces: Vec<LiveDisplaySurface>,
    source_width: u32,
    source_height: u32,
    media_kind: String,
    policy: PlaybackPolicy,
    clock: PlaybackClock,
    pause_reason: String,
    manual_pause: bool,
}

/// Running desktop-surface live session with background clock + policy ticks.
pub struct DesktopLiveSession {
    inner: Arc<Mutex<DesktopLiveSessionInner>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl DesktopLiveSession {
    fn start(
        surfaces: &[LiveDisplaySurface],
        source_width: u32,
        source_height: u32,
        media_kind: &str,
        policy: PlaybackPolicy,
        clock: PlaybackClock,
    ) -> Result<Self, BackendError> {
        let inner = Arc::new(Mutex::new(DesktopLiveSessionInner {
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
        }

        let stop = Arc::new(AtomicBool::new(false));
        let worker_inner = Arc::clone(&inner);
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("easel-desktop-live".into())
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

impl LiveWallpaperSession for DesktopLiveSession {
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
        let guard = self.inner.lock().map_err(|_| {
            BackendError::Platform("live session mutex poisoned during stop".into())
        })?;
        let wallpapers = posters_as_wallpapers(&guard.surfaces);
        let state = PlasmaWallpaperState::from_wallpapers(&wallpapers);
        publish_desktop_live_state(&state)?;
        Ok(())
    }
}

impl Drop for DesktopLiveSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn live_session_worker(inner: &Arc<Mutex<DesktopLiveSessionInner>>, stop: &Arc<AtomicBool>) {
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

fn apply_policy_to_clock(inner: &mut DesktopLiveSessionInner, sensors: LivePolicySensors) {
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

fn publish_session_state(inner: &DesktopLiveSessionInner) -> Result<(), BackendError> {
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
    publish_desktop_live_state(&state).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveMediaOutput, SourceUvRect};
    use easel_core::{DisplayId, LogicalRect};

    fn surface(dir: &Path) -> LiveDisplaySurface {
        let source = dir.join("clip.gif");
        let poster = dir.join("poster.png");
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
            source_width: 100,
            source_height: 100,
        }
    }

    #[test]
    fn backend_id_and_capabilities() {
        let backend = DesktopSurfaceLiveBackend;
        assert_eq!(backend.id(), "desktop-surface-live");
        let caps = backend.capabilities();
        assert!(caps.animated_images);
        assert!(caps.video);
        assert!(caps.shared_media_clock);
    }

    #[test]
    fn start_publishes_live_ipc() {
        let dir =
            std::env::temp_dir().join(format!("easel-desktop-live-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Redirect ProjectDirs via publishing to default path — just exercise start/stop.
        let surfaces = vec![surface(&dir)];
        let backend = DesktopSurfaceLiveBackend;
        let mut session = backend
            .start(
                &LiveWallpaperOutput::PerDisplay(surfaces),
                PlaybackPolicy::default(),
            )
            .expect("start");
        session.pause().expect("pause");
        session.resume().expect("resume");
        Box::new(session).stop().expect("stop");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
