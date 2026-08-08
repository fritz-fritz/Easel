# Platform support matrix

Capability-honest matrix for still apply, dynamic stills, and live hosts. Probes never
infer support from the OS name alone; see ADR 0003, ADR 0010, ADR 0011, and ADR 0014.

## Still wallpaper backends

| Session / OS | Backend id | Probe evidence | Per-display stills | Virtual desktop | Notes |
| --- | --- | --- | --- | --- | --- |
| KDE Plasma 6 | `plasma6` | `org.kde.plasmashell` reachable | yes | no | Prefers Easel Plasma plugin when installed. |
| XFCE | `xfce-xfconf` | `xfconf-query -c xfce4-desktop -l` succeeds | yes (XRandR geometry match) | yes | Stage 7.1. |
| GNOME family | `gnome-gsettings` | GNOME-session hint + readable `org.gnome.desktop.background` | yes (spanned composite of per-display crops; ADR 0012) | yes | Stage 7.2. |
| Generic X11 | `x11-feh` | `DISPLAY` set and `feh --version` succeeds; only if Plasma/XFCE/GNOME unavailable | yes | yes | Stage 7.1; root pixmap via feh. |
| Windows | `windows-idesktopwallpaper` | always on Windows builds | yes | yes | `IDesktopWallpaper`. |
| macOS | `macos` | always on macOS builds | yes | yes | System Events / AppKit still path. |

Automated coverage: `easel-platform` unit tests for XRandR parsing, XFCE/feh planning,
GNOME spanned compositing / session hints, and `select_wallpaper_backend` probe shape.
Manual validation: Cloud XFCE Apply via `xfce-xfconf` on the default single-monitor VNC
desktop; opt into `tools/dev/three-displays.sh` when exercising multi-monitor Apply.
GNOME Apply requires a real GNOME session (not available on the Cloud XFCE VM).

## Dynamic stills

Any selected still backend can receive polled frames. Native dynamic packages
(`BackendCapabilities::native_dynamic_bundle`) remain Plasma day/night and macOS HEIC
hosts; dense solar on Plasma uses still-frame IPC (ADR 0006–0008).

## Motion (GIF / video)

| Path | Backend id | Tier | Notes |
| --- | --- | --- | --- |
| KDE Plasma 6 + Easel plugin | `plasma6-live` | supported | Continuous shared-clock IPC (Stage 6). Preferred when available. |
| Any session with a still backend | `still-slideshow` | supported (slideshow) | Sample GIF/video into stills; timed `WallpaperBackend::apply` paced by `PlaybackPolicy::still_slideshow_interval_ms` (default 2 s, floor 500 ms — not video FPS; ADR 0014). Last frame persists after Easel exits. |
| Extraction / Apply failure | — | poster fallback | Single poster through the still backend. |

Public Windows/macOS wallpaper APIs remain still-image only (ADR 0010). Motion outside Plasma
uses those still APIs as a **poll-driven slideshow**, not WorkerW / private AppKit hosts.
Still backends cannot transition fast enough to simulate video; continuous under-icon playback
stays Plasma-plugin-only. Windows `SetSlideshow` is a folder playlist API and is not used for
per-display motion crops.

## Stage 7 remaining slices

- macOS packaging / distribution polish.
- Perspective / viewer correction + calibration UI.
- Workspace / activity / lock-screen only where stable public APIs exist.
- Optional: Windows equal-interval folder `SetSlideshow` for still rotations (not motion crops).
