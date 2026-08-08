# Dynamic and live wallpaper plan

## Capability definitions

Easel uses precise terms in its code, interface, and support matrix:

| Capability | Behavior | Runtime requirement |
| --- | --- | --- |
| Static | One rendered still remains active until replaced. | Public still-wallpaper backend. |
| Dynamic stills | A schedule, solar rule, or time-of-day timeline atomically replaces still frames. | Scheduler plus public still-wallpaper backend. |
| Animated image | A GIF, animated WebP, or similar local container presents as motion. | Continuous host (`LiveWallpaperBackend`) **or** still-backend slideshow (ADR 0014). |
| Video | A silent local video presents as motion. | Continuous host **or** still-backend slideshow (sample frames → timed still Apply). |

Native vendor dynamic wallpaper packages (Apple Dynamic Desktop HEIC, Plasma dynamic
HEIC/AVIF) are the preferred **interchange** format. Easel imports their schedule metadata
(`apple_desktop:solar` altitude/azimuth samples, `apr` appearance, `h24` time) into a portable
`DynamicStillSet`, retains the original package for provenance, and encodes per-display native
packages (crop every frame, then write Apple XMP HEIC) so physical spanning can be OS-hosted.
See ADR 0006.

## Feasibility assessment

Dynamic stills are feasible on any backend that can already apply a still image, and stronger
on platforms that can host a native dynamic package. Animated images and video prefer a
continuous live host when one exists (Stage 6 Plasma); otherwise they use still-backend
slideshow Apply (Stage 7.4 / ADR 0014).

| Platform/session | Dynamic stills | Animated/video host | Initial position |
| --- | --- | --- | --- |
| KDE Plasma 6 | Appearance → built-in day/night (`org.kde.image` + KNightTime). Dense solar → Rust evaluation + still frames via Easel `Plasma/Wallpaper` plugin IPC (ADR 0007 + 0008); no zzag required. | **Continuous:** Easel QML wallpaper plugin (`plasma6-live`) when installed. Else **still-slideshow** (ADR 0014). | First continuous live target. |
| Other Linux desktops | XFCE (`xfce-xfconf`), GNOME (`gnome-gsettings` spanned composite, ADR 0012), or generic X (`x11-feh`) when probed. | **Still-slideshow** through the probed still backend (ADR 0014). | Cloud XFCE validates XFCE Apply of sampled frames. |
| Windows | `IDesktopWallpaper` still-frame apply only (no public dynamic-HEIC API). | **Still-slideshow** via `IDesktopWallpaper` (ADR 0014). No WorkerW. | Public API stays still-only (ADR 0010). |
| macOS | Native Dynamic Desktop HEIC host (`native_dynamic_bundle`); System Events still apply as fallback. | **Still-slideshow** via System Events still Apply (ADR 0014). | Public `setDesktopImageURL` stays still-only (ADR 0010). |

The application must never advertise a live capability based only on the operating-system name.
It probes the current session and decoder, reports evidence in diagnostics, and falls back to the
poster frame when motion presentation fails.

## Live session design

One logical clock owns timing for a display group. Continuous hosts share decoded frames with
per-display UV crops and honor `maximum_frames_per_second`. Slideshow hosts compose each
sampled still once and Apply through the still backend on a configured **wallpaper poll**
interval (`still_slideshow_interval_ms`, default 2 s) — not container/video framerate. Both
paths avoid independent per-monitor players so bezels do not drift.

```mermaid
flowchart TD
    Source["Local media source"] --> Decoder["Qt Multimedia decoder"]
    Decoder --> Clock["Shared media clock"]
    Clock --> Compositor["Per-display crops + transforms"]
    Compositor --> Host["Capability-checked desktop host"]
    Poster["Rendered poster frame"] --> Host
```

The session lifecycle is `prepare → poster → play ↔ pause → stop`. Prepare validates the local
source, decoder, poster, surfaces, and policy without removing the current wallpaper. Playback
starts only after every requested surface is ready. A partial multi-monitor start is a failure.

Stage 6.7 models the shared timeline in Rust (`PlaybackClock`) and derives per-display crops
with the same planner as still posters (`plan_live_crops`). Stage 6.8 publishes that plan
plus `media_time_ms` into Plasma `active.json`; the plugin’s muted `AnimatedImage` /
`MediaPlayer` instances seek to the shared clock. Policy sensors (Stage 6.9) pause the
Rust clock; the plugin follows `paused` in IPC.

## Media and policy defaults

- Local files only for the initial motion implementation.
- Audio tracks are detected for diagnostics and always discarded.
- Loop playback and a 30 fps ceiling by default.
- Pause on battery and while a full-screen application is active by default.
- Pause on session lock and suspend; revalidate display topology and host surfaces on resume.
- Prefer hardware decoding when available, with measured software fallback rather than an
  unconditional guarantee.
- Extract or render a poster frame before Apply becomes available.
- Surface codec/container failures in the UI; do not silently transcode user media.

Streaming URLs are out of scope. They introduce network continuity, authentication, buffering,
cache, content changes, and provider-policy concerns that are independent of local playback.

## Delivery gates

A live backend moves from experimental to supported only after it demonstrates:

1. stable ownership below desktop icons across login, shell restart, workspace changes, and OS
   updates;
2. synchronized display crops within one presented frame;
3. bounded CPU, GPU, memory, and battery use on representative hardware;
4. correct pause/resume behavior for power, lock, sleep, and full-screen policy;
5. deterministic poster fallback after decoder, compositor, or host failure;
6. clear diagnostics for unavailable codecs and hardware acceleration.

## Primary references

- Qt Multimedia video overview: https://doc.qt.io/qt-6/videooverview.html
- Qt Quick `MediaPlayer`: https://doc.qt.io/qt-6/qml-qtmultimedia-mediaplayer.html
- KDE Plasma extension development: https://develop.kde.org/docs/plasma/
- Windows `IDesktopWallpaper`: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-idesktopwallpaper
- Windows `SetWallpaper`: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-idesktopwallpaper-setwallpaper
- macOS `setDesktopImageURL`: https://developer.apple.com/documentation/appkit/nsworkspace/setdesktopimageurl%28_%3Afor%3Aoptions%3A%29
