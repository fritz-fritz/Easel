# Platform support matrix

Capability-honest matrix for still apply, dynamic stills, and live hosts. Probes never
infer support from the OS name alone; see ADR 0003, ADR 0010, ADR 0011, and ADR 0013.

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

## Live wallpaper hosts

| Session / OS | Backend id | Supported | Notes |
| --- | --- | --- | --- |
| KDE Plasma 6 + Easel plugin | `plasma6-live` | yes | Shared-clock IPC (Stage 6). |
| Other Linux desktops | — | no | Poster fallback via still backend when one exists. |
| Windows | — | no | ADR 0010. |
| macOS | — | no | ADR 0010. |

## Stage 7 remaining slices

- macOS packaging / distribution polish.
- Perspective / viewer correction + calibration UI.
- Workspace / activity / lock-screen only where stable public APIs exist.
- Non-Plasma live hosts (separate feasibility ADR when candidates exist).

Dynamic stills are feature-complete on every still backend (Stage 7.3 / ADR 0013).
