# ADR 0007: Plasma dynamic host — built-in day/night vs community solar

- Status: accepted
- Date: 2026-07-18

## Context

Stage 5 treats Apple Dynamic Desktop HEIC as the interchange format and prefers native
OS hosting when `BackendCapabilities::native_dynamic_bundle` is true. On KDE Plasma the
landscape is split, and “dynamic wallpaper” means different things depending on which
stack is installed.

### What shipped as standard (Plasma 6.4+)

Plasma 6.4 added **time-of-day / day-night wallpapers** to stock `org.kde.image`
(Vlad Zahorodnii; covered in This Week in Plasma and his dark-mode write-up). Compatible
packages live under `~/.local/share/wallpapers` with:

- `contents/images/` — light frame(s), named by resolution (`5120x2880.png`)
- `contents/images_dark/` — dark frame(s)
- `metadata.json` (`KPlugin` id/name; optional `X-KDE-CrossFade`)

`DynamicMode=1` switches light/dark from the **sunrise/sunset** schedule provided by
**KNightTime** (`knighttimed`), using geolocation when available, else fixed morning/evening
times. This is the **standard** Plasma Wayland path today. It is **two frames only** — not
Apple-style dense altitude/azimuth sampling.

Plasma 6.5 moved day/night cycle configuration to its own System Settings page so Night
Light, wallpapers, and (later) theme switching share one schedule. That does not expand
the wallpaper format beyond light/dark.

### Why stock Plasma is not Apple-parity solar

The same author previously shipped
[`com.github.zzag.dynamic`](https://github.com/zzag/plasma5-wallpapers-dynamic)
(community plugin; master targets Plasma 6). It supports solar elevation/azimuth engines
and HEIC→AVIF conversion tooling (`dynamicwallpaperconverter` /
`kdynamicwallpaperbuilder`). Upstream deliberately took only the **day/night** subset into
Plasma Workspace: the full plugin is large, needs special tooling, and its multi-frame
package format was judged too cumbersome for default shipping — while remaining excellent
for dense 5K/8K solar sets.

So on a stock Plasma Wayland install:

| Capability | Available? |
| --- | --- |
| Light/dark package + KNightTime | Yes (built-in) |
| Dense solar / h24 HEIC host | No (needs zzag) |
| Apple HEIC import as native host | Only via zzag conversion |

### Options evaluated for Easel

| Option | Pros | Cons | Verdict |
| --- | --- | --- | --- |
| **A. Built-in day/night packages** | Zero extra deps; Wayland-native; matches Plasma 6.4+ UX | Appearance / two-frame only | **Use for `Appearance` sets** |
| **B. Community zzag HEIC/AVIF** | Closest to Apple solar/h24 hosting | Not installed by default; optional probe | **Use when plugin present** for dense solar |
| **C. Easel still poller** | Correct on every Plasma; no plugin | Easel must stay running; not “OS-hosted” | **Fallback** for dense solar without zzag |
| **D. Reduce dense solar → day/night** | Stock hosting for more imports | Loses intermediate frames | Future optional quality trade-off |
| **E. Claim stock Plasma = Apple Dynamic Desktop** | Marketing simplicity | Factually wrong | **Rejected** |

Windows still has no public dynamic-HEIC API. macOS hosts Apple HEIC natively.

## Decision

| Still-set kind | Plasma host | Fallback |
| --- | --- | --- |
| `Appearance` (light/dark) | Built-in day/night package → `org.kde.image` + `DynamicMode=1` | Still poller |
| `SolarPosition` / dense `TimeOfDay` | Community zzag HEIC/AVIF **if** plugin present | Still poller |
| Authored solar sunrise/sunset keys | Still poller (or day/night reduction later) | Still poller |

Apple HEIC remains the interchange import/export format. Plasma day/night packages are a
**derived apply output**, like per-display HEIC crops on macOS.

`PlasmaBackend::native_dynamic_bundle` is **true** because appearance sets can be
OS-hosted without extras. Automation must still fall back to the still poller when the
chosen native format is unavailable (dense solar without zzag).

**Superseding direction (ADR 0008):** Easel will ship its own `Plasma/Wallpaper` plugin so
dense solar, live media, and managed multi-display crops are OS-hosted without depending
on zzag. Built-in day/night remains the preferred zero-daemon path for Appearance sets.

## Consequences

- Appearance-keyed Mojave-style imports can be OS-hosted on stock Plasma Wayland.
- Dense solar sets keep correct behavior via Easel's poller on stock Plasma until the
  Easel wallpaper plugin (ADR 0008) is wired; zzag remains an optional interim host.
- Docs and Compose copy must not claim Apple-parity solar hosting on stock Plasma.
- Future work: Easel plugin host (0008); optional AVIF writer; optional reduction of dense
  solar → day/night for built-in-only users.

## References

- https://blogs.kde.org/2025/05/24/this-week-in-plasma-time-of-day-wallpapers/ (Plasma 6.4)
- https://blog.vladzahorodnii.com/2025/08/11/dark-mode-improvements-in-plasma/
- https://blogs.kde.org/2025/07/12/this-week-in-plasma-tablet-dials-and-day/night-cycles/ (KNightTime settings)
- https://invent.kde.org/plasma/plasma-workspace (org.kde.image + KNightTime)
- https://invent.kde.org/plasma/knighttime
- https://github.com/zzag/plasma5-wallpapers-dynamic
- `docs/adr/0006-apple-heic-dynamic-interchange.md`
- `docs/adr/0008-plasma-wallpaper-plugin-host.md`
