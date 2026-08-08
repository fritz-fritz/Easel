# Platform support matrix

Capability-honest matrix for still apply, dynamic stills, and live hosts. Probes never
infer support from the OS name alone; see ADR 0003, ADR 0010, and ADR 0011.

## Still wallpaper backends

| Session / OS | Backend id | Probe evidence | Per-display stills | Virtual desktop | Notes |
| --- | --- | --- | --- | --- | --- |
| KDE Plasma 6 | `plasma6` | `org.kde.plasmashell` reachable | yes | no | Prefers Easel Plasma plugin when installed. |
| XFCE | `xfce-xfconf` | `xfconf-query -c xfce4-desktop -l` succeeds | yes (XRandR geometry match) | yes | Stage 7.1. |
| Generic X11 | `x11-feh` | `DISPLAY` set and `feh --version` succeeds; only if Plasma/XFCE unavailable | yes | yes | Stage 7.1; root pixmap via feh. |
| GNOME family | — | not selected yet | — | — | Stage 7.2; no stable public per-monitor still API. |
| Windows | `windows-idesktopwallpaper` | always on Windows builds | yes | yes | `IDesktopWallpaper`. |
| macOS | `macos` | always on macOS builds | yes | yes | System Events / AppKit still path. |

Automated coverage: `easel-platform` unit tests for XRandR parsing, XFCE/feh planning, and
`select_wallpaper_backend` probe shape. Manual validation on the Cloud XFCE multi-monitor
session (three-displays) exercises live Apply through `xfce-xfconf`.

## Dynamic stills

Any selected still backend can receive polled frames. Native dynamic packages
(`BackendCapabilities::native_dynamic_bundle`) remain Plasma day/night and macOS HEIC
hosts; dense solar on Plasma uses still-frame IPC (ADR 0006–0008).

## Live wallpaper hosts

| Session / OS | Backend id | Supported | Notes |
| --- | --- | --- | --- |
| KDE Plasma 6 + Easel plugin | `plasma6-live` | yes | Shared-clock IPC (Stage 6). |
| Other Linux desktops | — | no | Poster fallback via still backend when one exists. |
| Windows | — | no | ADR 0010. |
| macOS | — | no | ADR 0010. |

## Stage 7 remaining slices

- 7.2 GNOME still adapter (or explicit virtual-desktop-only tier).
- macOS packaging / distribution polish.
- Perspective / viewer correction + calibration UI.
- Workspace / activity / lock-screen only where stable public APIs exist.
- Non-Plasma live hosts (separate feasibility ADR when candidates exist).
