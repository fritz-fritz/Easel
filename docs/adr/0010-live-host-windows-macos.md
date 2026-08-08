# ADR 0010: Windows and macOS live-host feasibility (Stage 6 spikes)

- Status: accepted
- Date: 2026-08-08

## Context

Stage 6 requires capability-honest live wallpaper probing (ADR 0003). KDE Plasma has a
validated host path via Easel’s `Plasma/Wallpaper` plugin. Windows and macOS need explicit
feasibility spikes before any experimental live backend is enabled.

## Decision

### Windows

Public wallpaper APIs (`IDesktopWallpaper::SetWallpaper`, `SystemParametersInfo(SPI_SETDESKWALLPAPER)`)
accept **still image paths only**. There is no supported COM/WinRT contract to attach a
silent video or animated-image surface beneath desktop icons while preserving Explorer’s
icon layer.

Third-party “live wallpaper” tools typically inject a WorkerW child window behind icons.
That approach is undocumented, breaks across Explorer/shell updates, and fails Easel’s
stability gates (login, shell restart, multi-monitor crops, power budgets).

**Position:** Windows live host remains **unsupported**. Apply uses poster-frame fallback
through `IDesktopWallpaper`. `probe_live_wallpaper_backend` reports this ADR explicitly.

### macOS

`NSWorkspace.setDesktopImageURL(_:for:options:)` and related System Events paths are
**still-image oriented**. Native Dynamic Desktop HEIC hosting covers dynamic *stills*
(`native_dynamic_bundle`) but not continuous animated-image or video playback under the
desktop icon layer.

Private or ScreenSaver-based approaches are out of scope for a supported backend.

**Position:** macOS live host remains **unsupported**. Dynamic still HEIC hosting continues;
live Apply uses poster fallback. Probe text cites this ADR.

## Consequences

- Stage 6 exit criteria are met on **Plasma + Easel plugin** as the first supported live
  backend; Windows/macOS stay poster-fallback with documented reasons.
- Revisit only if Microsoft or Apple publish a stable public live-surface API that can
  share one media clock across displays.

## References

- `docs/adr/0003-dynamic-and-live-wallpapers.md`
- https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-idesktopwallpaper
- https://developer.apple.com/documentation/appkit/nsworkspace/setdesktopimageurl(_:for:options:)
