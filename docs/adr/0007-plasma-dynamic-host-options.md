# ADR 0007: Plasma dynamic host — built-in day/night vs community solar

- Status: accepted
- Date: 2026-07-18

## Context

Stage 5 treats Apple Dynamic Desktop HEIC as the interchange format and prefers native
OS hosting when `BackendCapabilities::native_dynamic_bundle` is true. On KDE Plasma the
landscape is split:

1. **Built-in Plasma 6 day/night wallpapers** (Vlad Zahorodnii / Plasma Workspace,
   KNightTime, shipped with recent Plasma 6.x): regular wallpaper packages under
   `~/.local/share/wallpapers` with `contents/images` + `contents/images_dark` and
   `metadata.json` (`X-KDE-CrossFade` optional). `org.kde.image` switches light/dark at
   sunrise/sunset (or fixed times) via `knighttimed`. This is **two frames only** — not
   Apple-style dense altitude/azimuth samples.

2. **Community plugin** [`com.github.zzag.dynamic`](https://github.com/zzag/plasma5-wallpapers-dynamic)
   (same author; basis for the upstream day/night work): full solar elevation/azimuth and
   day-night engines, consuming HEIC/AVIF packages built with `kdynamicwallpaperbuilder` /
   `dynamicwallpaperconverter` from Apple HEIC. Not installed by default.

3. **Easel still poller**: evaluates `DynamicStillSet` in-process and applies cropped PNGs
   through `org.kde.image`. Correct for dense solar/h24 on stock Plasma; works without
   plugins.

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

## Consequences

- Appearance-keyed Mojave-style imports can be OS-hosted on stock Plasma Wayland.
- Dense solar sets keep correct behavior via Easel's poller on stock Plasma; users who
  install zzag get true native solar packages.
- Docs and Compose copy must not claim Apple-parity solar hosting on stock Plasma.
- Future work: optional AVIF writer matching `kdynamicwallpaperbuilder` manifests; optional
  reduction of dense solar → day/night for users who prefer built-in hosting only.

## References

- https://blog.vladzahorodnii.com/2025/08/11/dark-mode-improvements-in-plasma/
- https://invent.kde.org/plasma/plasma-workspace (org.kde.image + KNightTime)
- https://github.com/zzag/plasma5-wallpapers-dynamic
- `docs/adr/0006-apple-heic-dynamic-interchange.md`
