# ADR 0014: Motion as still-backend slideshow

- Status: accepted (supersedes app-owned desktop-surface approach)
- Date: 2026-08-08

## Context

Stage 6 delivered continuous live playback on Plasma via the Easel wallpaper plugin
(ADR 0008). ADR 0010 correctly found no public Windows/macOS API for video under the
icon layer. Stage 7.1–7.2 added still backends on XFCE, GNOME, feh, Windows, and macOS.

An earlier draft of this ADR proposed app-owned always-on-bottom Qt windows
(`desktop-surface-live`). That path does not use native wallpaper APIs, dies when Easel
exits, and fights DE icon/backdrop stacking. The more supportable cross-platform approach
is to **sample GIF/video into still frames and Apply them through the existing still
backends**, reusing the same probe chain and persistence model as static wallpapers.

Still wallpaper APIs (`xfconf-query`, `gsettings`, `IDesktopWallpaper::SetWallpaper`,
System Events `set picture`, `feh`) are **settings-channel writes**, not media pipelines.
They are too slow and too expensive to simulate video framerate. Private hosts (WorkerW,
private AppKit layering) would be required for true under-icon video; Easel rejects those
as default backends (ADR 0010). Continuous decode remains Plasma-plugin-only.

## Decision

1. **Preference:** `plasma6-live` when Plasma + Easel plugin are installed (continuous,
   shared-clock host — unchanged). `PlaybackPolicy::maximum_frames_per_second` applies
   only to this continuous path.

2. **Otherwise:** `still-slideshow` — extract bounded frames (GIF via `image` /
   `extract_gif_frames`; video via optional host `ffmpeg` sampled to match the poll),
   compose per-display crops, and call `WallpaperBackend::apply(PerDisplay)` on a timed
   worker.

3. **Cadence is a poll interval, not FPS.** Applies are paced by
   `PlaybackPolicy::still_slideshow_interval_ms` (default **2000 ms**, clamp
   **500–60 000 ms**, rate-scaled via `effective_still_slideshow_interval_ms`). Compose
   exposes presets (“Wallpaper poll”). Container/GIF frame delays are **not** used as
   Apply cadence. Easel must not chase 24/30 fps through still backends.

4. **Native slideshow sets:** Windows exposes `IDesktopWallpaper::SetSlideshow` for
   equal-interval folder playlists. That API cannot express Easel’s per-display crops or
   GIF-derived sequences, so timed `SetWallpaper` remains the motion path. Prefer a native
   slideshow set later only for equal-interval still rotations of one shared folder.
   Other platforms have no useful public wallpaper playlist for this use case.

5. **Probe:** `probe_live_wallpaper_backend` reports `still-slideshow` whenever a still
   backend exists. Continuous `LiveWallpaperBackend::start` remains Plasma-only;
   slideshow sessions live in the desktop process and implement `LiveWallpaperSession`
   (pause/resume/stop + policy sensors).

6. **Failure:** if extraction or Apply fails, seed/keep a single poster through the still
   backend (Stage 6.6 behavior). Compose “Poster frame only” skips the slideshow entirely.

7. **Out of scope:** WorkerW, private AppKit layering, and app-owned desktop windows as
   default motion hosts. Revisit continuous OS-integrated hosts only if vendors publish
   stable public live-surface APIs.

## Consequences

- GIF/video motion works on every still backend (XFCE Cloud VM included) using native
  wallpaper settings channels, as a slow slideshow rather than video.
- Last applied frame remains after Easel exits (still backend persistence).
- Users can trade responsiveness vs DE health via Wallpaper poll; backends may still lag
  behind the requested interval (especially GNOME multi-monitor spanned composites).
- First-loop projective / perspective re-rasters are expected when viewer pose or panel
  angles are enabled; the slideshow cache fingerprint includes pose + angles so
  mid-session calibration invalidates correctly (Stage 7.7).
- Plasma keeps the high-fidelity continuous path when the plugin is installed.
- Private APIs are **not** required for the supported slideshow tier; they would only be
  needed for true video-rate under-icon playback, which remains unsupported.

## References

- `docs/adr/0008-plasma-wallpaper-plugin-host.md`
- `docs/adr/0010-live-host-windows-macos.md`
- `docs/adr/0011-linux-still-wallpaper-backends.md`
- `docs/LIVE_WALLPAPERS.md`
- `docs/PLATFORM_SUPPORT.md`
