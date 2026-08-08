# ADR 0013: Session presentation-support probe for all still backends

- Status: accepted
- Date: 2026-08-08

## Context

Stage 7.1–7.2 added XFCE, GNOME, and feh still backends beside Plasma, Windows, and macOS.
Dynamic stills are not a separate OS API on most of those hosts: they reuse the still
`WallpaperBackend` as a frame poller, with native packages only on Plasma Appearance and
macOS HEIC (ADR 0005 / 0006). PRODUCT requires reporting static, dynamic-still, animated-image,
and video support separately for the active backend. Live already had `LiveBackendProbe`;
still/dynamic reporting was implicit and easy to under-document after the Linux still breadth.

## Decision

1. Add `probe_wallpaper_backend()` returning `WallpaperBackendProbe` with backend id,
   capabilities, and a `DynamicStillsHost` strategy (`StillPoller` or
   `NativeBundleWithPollerFallback`).
2. Add `probe_presentation_support()` combining still + live probes into the four PRODUCT
   modes for CLI (`easel status`) and Compose media-mode capability hints.
3. Keep apply behavior unchanged: native packages when `native_dynamic_bundle` and the still
   set allow; otherwise the still-frame poller on every selected still backend. Do not invent
   native dynamic hosts for GNOME/XFCE/Windows/feh.

## Consequences

- Support matrix (`docs/PLATFORM_SUPPORT.md`) lists dynamic stills per backend with apply path.
- Operators can see poller vs native strategy without reading ADRs.
- Continuous live remains Plasma-plugin-only; Stage 7.4 (ADR 0014) reports animated/video via
  `still-slideshow` when a still backend exists, so `probe_presentation_support()` can show
  motion support without a continuous host.

## References

- `docs/PRODUCT.md` (Primary experiences — Dynamic and live wallpaper)
- `docs/PLATFORM_SUPPORT.md`
- `docs/adr/0005-dynamic-stills.md`
- `docs/adr/0006-apple-heic-dynamic-interchange.md`
- `docs/adr/0014-motion-still-slideshow.md`
