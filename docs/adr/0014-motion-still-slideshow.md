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

## Decision

1. **Preference:** `plasma6-live` when Plasma + Easel plugin are installed (continuous,
   shared-clock host — unchanged).

2. **Otherwise:** `still-slideshow` — extract bounded frames (GIF via `image` /
   `extract_gif_frames`; video via optional host `ffmpeg`), compose per-display crops,
   and call `WallpaperBackend::apply(PerDisplay)` on a timed worker. Frame delays are
   clamped (`MIN_SLIDESHOW_DELAY_MS`) so xfconf/gsettings/IDesktopWallpaper are not
   thrashed.

3. **Probe:** `probe_live_wallpaper_backend` reports `still-slideshow` whenever a still
   backend exists. Continuous `LiveWallpaperBackend::start` remains Plasma-only;
   slideshow sessions live in the desktop process and implement `LiveWallpaperSession`
   (pause/resume/stop + policy sensors).

4. **Failure:** if extraction or Apply fails, seed/keep a single poster through the still
   backend (Stage 6.6 behavior).

5. **Out of scope:** WorkerW, private AppKit layering, and app-owned desktop windows as
   default motion hosts. ADR 0010 stands for public OS video wallpaper APIs.

## Consequences

- GIF/video motion works on every still backend (XFCE Cloud VM included) using native
  wallpaper settings channels.
- Last applied frame remains after Easel exits (still backend persistence).
- Motion is a slideshow approximation, not true video decode under the desktop icons;
  frame rate is intentionally capped for backend health.
- Plasma keeps the high-fidelity continuous path when the plugin is installed.

## References

- `docs/adr/0008-plasma-wallpaper-plugin-host.md`
- `docs/adr/0010-live-host-windows-macos.md`
- `docs/adr/0011-linux-still-wallpaper-backends.md`
- `docs/LIVE_WALLPAPERS.md`
- `docs/PLATFORM_SUPPORT.md`
