// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Motion-as-slideshow session: timed still Apply through the native wallpaper backend.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use easel_core::{LoopMode, PlaybackPolicy, Profile};
use easel_platform::{
    BackendError, LiveWallpaperSession, pause_reason_for, probe_live_policy_sensors,
};
use easel_render::{
    CompositionSettings, MAX_MOTION_FRAMES, MotionFrame, RasterJob, RenderPurpose, RenderRequest,
    extract_gif_frames,
};

use crate::apply_service::apply_cache_dir;
use crate::automation_session::automation_store;
use crate::display_session;

const TICK: Duration = Duration::from_millis(50);
const MAX_VIDEO_FRAMES: usize = 24;

/// Builds a slideshow session for GIF/video through still-backend Apply.
pub fn start_slideshow_session(
    source: &Path,
    profile: &Profile,
) -> Result<Box<dyn LiveWallpaperSession>, String> {
    let frames = extract_motion_frames(source)?;
    if frames.is_empty() {
        return Err("slideshow produced no frames".into());
    }
    let session = SlideshowSession::start(frames, profile.clone(), profile.playback)?;
    Ok(Box::new(session))
}

fn extract_motion_frames(source: &Path) -> Result<Vec<MotionFrame>, String> {
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let out = apply_cache_dir().join("slideshow-frames").join(format!(
        "{}-{}",
        std::process::id(),
        source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("motion")
    ));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).map_err(|error| error.to_string())?;

    if extension == "gif" {
        return extract_gif_frames(source, &out, MAX_MOTION_FRAMES)
            .map_err(|error| error.to_string());
    }
    extract_video_frames_ffmpeg(source, &out, MAX_VIDEO_FRAMES)
}

fn extract_video_frames_ffmpeg(
    source: &Path,
    output_dir: &Path,
    max_frames: usize,
) -> Result<Vec<MotionFrame>, String> {
    // Optional host toolchain — same policy as docs (no required ffmpeg crate dep).
    let status = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(source)
        .args([
            "-vf",
            "fps=2,scale='min(1920,iw)':-2",
            "-frames:v",
            &max_frames.to_string(),
            "-start_number",
            "0",
        ])
        .arg(output_dir.join("frame-%04d.png"))
        .status()
        .map_err(|error| {
            format!(
                "ffmpeg unavailable for video slideshow ({error}); index a poster or install ffmpeg"
            )
        })?;
    if !status.success() {
        return Err(format!("ffmpeg frame extract failed ({status})"));
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(output_dir)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        })
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err("ffmpeg produced no PNG frames".into());
    }
    let delay_ms = 500u64;
    let mut media_time_ms = 0u64;
    let mut frames = Vec::with_capacity(paths.len());
    for path in paths {
        frames.push(MotionFrame {
            path,
            delay_ms,
            media_time_ms,
        });
        media_time_ms = media_time_ms.saturating_add(delay_ms);
    }
    Ok(frames)
}

struct SlideshowInner {
    frames: Vec<MotionFrame>,
    profile: Profile,
    policy: PlaybackPolicy,
    index: usize,
    remaining_ms: u64,
    manual_pause: bool,
    /// Cached composed wallpapers per frame index (invalidated on stop only).
    raster_cache: Vec<Option<Vec<(easel_core::DisplayId, PathBuf, easel_core::LogicalRect)>>>,
}

pub struct SlideshowSession {
    inner: Arc<Mutex<SlideshowInner>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl SlideshowSession {
    fn start(
        frames: Vec<MotionFrame>,
        profile: Profile,
        policy: PlaybackPolicy,
    ) -> Result<Self, String> {
        let first_delay = frames[0].delay_ms;
        let raster_cache = vec![None; frames.len()];
        let inner = Arc::new(Mutex::new(SlideshowInner {
            frames,
            profile,
            policy,
            index: 0,
            remaining_ms: first_delay,
            manual_pause: false,
            raster_cache,
        }));

        {
            let mut guard = inner
                .lock()
                .map_err(|_| "slideshow mutex poisoned during start".to_owned())?;
            apply_frame_locked(&mut guard)?;
        }

        let stop = Arc::new(AtomicBool::new(false));
        let worker_inner = Arc::clone(&inner);
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("easel-still-slideshow".into())
            .spawn(move || slideshow_worker(&worker_inner, &worker_stop))
            .map_err(|error| format!("failed to start slideshow worker: {error}"))?;

        Ok(Self {
            inner,
            stop,
            worker: Some(worker),
        })
    }
}

impl LiveWallpaperSession for SlideshowSession {
    fn pause(&mut self) -> Result<(), BackendError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| BackendError::Platform("slideshow mutex poisoned during pause".into()))?;
        guard.manual_pause = true;
        Ok(())
    }

    fn resume(&mut self) -> Result<(), BackendError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| BackendError::Platform("slideshow mutex poisoned during resume".into()))?;
        guard.manual_pause = false;
        Ok(())
    }

    fn stop(mut self: Box<Self>) -> Result<(), BackendError> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        Ok(())
    }
}

impl Drop for SlideshowSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn slideshow_worker(inner: &Arc<Mutex<SlideshowInner>>, stop: &Arc<AtomicBool>) {
    let mut last = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        thread::sleep(TICK);
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let now = Instant::now();
        let delta = u64::try_from(now.saturating_duration_since(last).as_millis()).unwrap_or(0);
        last = now;

        let Ok(mut guard) = inner.lock() else {
            break;
        };
        let sensors = probe_live_policy_sensors();
        let paused = guard.manual_pause || pause_reason_for(&guard.policy, &sensors).is_some();
        if paused {
            continue;
        }
        if delta >= guard.remaining_ms {
            let overflow = delta - guard.remaining_ms;
            advance_frame(&mut guard);
            if overflow < guard.remaining_ms {
                guard.remaining_ms -= overflow;
            }
            let _ = apply_frame_locked(&mut guard);
        } else {
            guard.remaining_ms -= delta;
        }
    }
}

fn advance_frame(inner: &mut SlideshowInner) {
    let next = inner.index + 1;
    if next >= inner.frames.len() {
        match inner.policy.loop_mode {
            LoopMode::Loop => {
                inner.index = 0;
                inner.remaining_ms = inner.frames[0].delay_ms;
            }
            LoopMode::Once => {
                // Hold final frame.
                inner.remaining_ms = u64::MAX / 4;
            }
        }
    } else {
        inner.index = next;
        inner.remaining_ms = inner.frames[inner.index].delay_ms;
    }
}

fn apply_frame_locked(inner: &mut SlideshowInner) -> Result<(), String> {
    let index = inner.index;
    if let Some(cached) = inner.raster_cache.get(index).and_then(|slot| slot.as_ref()) {
        return push_wallpapers(cached);
    }
    let source = inner.frames[index].path.clone();
    let wallpapers = compose_frame(&source, &inner.profile)?;
    push_wallpapers(&wallpapers)?;
    if let Some(slot) = inner.raster_cache.get_mut(index) {
        *slot = Some(wallpapers);
    }
    Ok(())
}

fn compose_frame(
    source: &Path,
    profile: &Profile,
) -> Result<Vec<(easel_core::DisplayId, PathBuf, easel_core::LogicalRect)>, String> {
    use easel_core::resolve_displays;

    let live = display_session::current_displays();
    if live.is_empty() {
        return Err("no displays available".into());
    }
    let policy = automation_store()?.hotplug_policy().clone();
    let resolution = resolve_displays(profile, &live, &policy);
    if !resolution.should_apply {
        return Err(resolution.reason);
    }
    let displays = resolution.active_displays;
    let mut request_profile = profile.clone();
    request_profile.displays = displays.iter().map(|display| display.id).collect();
    let outputs = RasterJob {
        request: RenderRequest {
            source_path: source.to_path_buf(),
            displays: displays.clone(),
            composition: CompositionSettings::from_profile(&request_profile),
            purpose: RenderPurpose::LivePosterFrame,
        },
        output_dir: apply_cache_dir().join(format!("slideshow-{}", std::process::id())),
    }
    .execute()
    .map_err(|error| error.to_string())?;

    let mut wallpapers = Vec::with_capacity(outputs.len());
    for output in outputs {
        let logical_rect = displays
            .iter()
            .find(|display| display.id == output.display_id)
            .map(|display| display.logical_rect)
            .ok_or_else(|| "display missing for slideshow raster".to_owned())?;
        wallpapers.push((output.display_id, output.path, logical_rect));
    }
    Ok(wallpapers)
}

fn push_wallpapers(
    wallpapers: &[(easel_core::DisplayId, PathBuf, easel_core::LogicalRect)],
) -> Result<(), String> {
    use easel_platform::{DisplayWallpaper, WallpaperOutput, select_wallpaper_backend};

    let backend = select_wallpaper_backend().map_err(|error| error.to_string())?;
    let payload = wallpapers
        .iter()
        .map(|(id, path, rect)| DisplayWallpaper {
            display_id: *id,
            path: path.clone(),
            logical_rect: *rect,
        })
        .collect();
    backend
        .apply(&WallpaperOutput::PerDisplay(payload))
        .map_err(|error| error.to_string())
}
