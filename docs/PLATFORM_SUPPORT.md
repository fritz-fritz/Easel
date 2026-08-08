# Platform support matrix

Capability-honest matrix for still apply, dynamic stills, and live hosts. Probes never
infer support from the OS name alone; see ADR 0003, ADR 0010, ADR 0011, ADR 0013, and ADR 0014.

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
GNOME spanned compositing / session hints, `probe_wallpaper_backend` /
`probe_presentation_support`, and `select_wallpaper_backend` probe shape.
Manual validation: Cloud XFCE Apply via `xfce-xfconf` on the default single-monitor VNC
desktop; opt into `tools/dev/three-displays.sh` when exercising multi-monitor Apply.
GNOME Apply requires a real GNOME session (not available on the Cloud XFCE VM).

## Dynamic stills

Any selected still backend can receive polled dynamic-still frames. Native packages
(`BackendCapabilities::native_dynamic_bundle`) are preferred when available; dense solar on
Plasma uses still-frame IPC instead (ADR 0006–0008). Session diagnostics come from
`probe_presentation_support()` (CLI `easel status`, Compose media-mode hints).

| Session / OS | Backend id | Dynamic stills | Native package host | Apply path |
| --- | --- | --- | --- | --- |
| KDE Plasma 6 | `plasma6` | yes | yes (Appearance day/night) | Native for Appearance; still poller / plugin IPC for dense solar/h24 |
| XFCE | `xfce-xfconf` | yes | no | Still-frame poller |
| GNOME family | `gnome-gsettings` | yes | no | Still-frame poller (spanned composite for multi-monitor) |
| Generic X11 | `x11-feh` | yes | no | Still-frame poller |
| Windows | `windows-idesktopwallpaper` | yes | no | Still-frame poller (ADR 0006) |
| macOS | `macos` | yes | yes (Dynamic Desktop HEIC) | Native HEIC host; System Events still poller as fallback |

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
- Per-display monitor tilt/yaw and a dedicated perspective calibration wizard
  (Stage 7.5 landed distance-first global viewer pose + Compose Physical controls; ADR 0015).
- Workspace / activity / lock-screen only where stable public APIs exist.
- Optional: Windows equal-interval folder `SetSlideshow` for still rotations (not motion crops).

Dynamic stills are feature-complete on every still backend (Stage 7.3 / ADR 0013).
Motion outside Plasma uses still-backend slideshow Apply (Stage 7.4 / ADR 0014).
Perspective correction applies to still Apply / posters / slideshow frames; continuous
Plasma live UV remains axis-aligned until a projective live path lands (ADR 0015).
