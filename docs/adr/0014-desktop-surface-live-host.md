# ADR 0014: App-owned desktop-surface live host (experimental)

- Status: accepted
- Date: 2026-08-08

## Context

Stage 6 delivered a **supported** live host on KDE Plasma via the Easel wallpaper plugin
(ADR 0008). ADR 0010 correctly concluded that Windows and macOS expose no public OS API to
attach continuous video/GIF under the icon layer. After Stage 7.1–7.2 still backends, every
desktop can Apply posters, but Compose Animated/video still degraded to posters outside Plasma.

Users need GIF/video backgrounds on XFCE (including the Cloud VM), GNOME, Windows, and macOS.
Inventing WorkerW / private AppKit hosts as “supported OS wallpaper APIs” would violate
`ARCHITECTURE.md`. An **application-owned** surface that the desktop process draws is honest:
it is not an OS wallpaper contract, requires Easel to keep running, and icon stacking is
DE-dependent.

## Decision

1. Add `desktop-surface-live` implementing `LiveWallpaperBackend` / `LiveWallpaperSession`.
   It publishes the same live IPC document shape as Plasma (`PlasmaWallpaperState` v2) to
   `$XDG_DATA_HOME/easel/desktop-live/active.json` (platform ProjectDirs equivalent elsewhere).

2. `easel-desktop` loads `LiveDesktopSurfaces.qml`: one always-on-bottom / desktop-type window
   per `Qt.application.screens`, muted `AnimatedImage` / `MediaPlayer` with UV crops and shared
   `media_time_ms` from the Rust clock worker.

3. Probe preference: `plasma6-live` when Plasma + Easel plugin are present (**supported**);
   otherwise `desktop-surface-live` (**experimental**, ADR 0014). ADR 0010 remains correct that
   public Win/macOS wallpaper APIs are still-image only — this host does not claim otherwise.

4. Apply still seeds posters through the still backend, then starts the live session. If live
   `start` fails, fall back to posters with the error noted.

5. Delivery gates in `LIVE_WALLPAPERS.md` still apply before promoting `desktop-surface-live`
   from experimental to supported.

## Consequences

- GIF/video Apply works on Cloud XFCE and other sessions while Easel is running.
- Closing Easel stops app-owned live surfaces; posters remain via the still backend.
- Icon/backdrop stacking may be imperfect on some DEs — reported in probe reason text.
- Windows WorkerW and macOS private ScreenSaver hosts stay out of scope.

## References

- `docs/adr/0008-plasma-wallpaper-plugin-host.md`
- `docs/adr/0010-live-host-windows-macos.md`
- `docs/LIVE_WALLPAPERS.md`
- `docs/PLATFORM_SUPPORT.md`
