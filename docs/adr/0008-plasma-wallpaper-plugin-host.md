# ADR 0008: Easel Plasma wallpaper plugin as the OS host

- Status: accepted
- Date: 2026-07-19

## Context

Stock Plasma 6.4+ can host **day/night** wallpaper packages (`images` + `images_dark`)
via `org.kde.image` and KNightTime (system theme or sunrise/sunset). That covers
Appearance-keyed still sets (ADR 0007) but not dense solar/h24 schedules, live media, or
Easel-managed multi-display crops without keeping the desktop app’s still poller running.

Plasma’s intended extension point for richer wallpaper behavior is a **`Plasma/Wallpaper`
QML package** (`KPackageStructure: Plasma/Wallpaper`, root `WallpaperItem`). Community
plugins (zzag dynamic, video wallpaper plugins) already use this path. Stage 6 had already
named a Plasma QML plugin for live media; using the same host for dynamic stills gives
tighter OS integration while Easel’s Compose/Library GUI remains the management surface.

## Decision

1. **Ship an Easel wallpaper plugin** (`net.fritztech.easel.wallpaper`) installed under
   `~/.local/share/plasma/wallpapers/<id>/` (and system prefix when packaged). Plasma
   treats Easel as the active wallpaper engine per desktop/containment.
2. **Keep `easel-desktop` / `easel-cli` as the control plane.** The plugin renders and
   reacts; library, schedules, spanning layout, and import stay in the GUI/CLI. The plugin
   must not become a second settings UI beyond minimal Plasma wallpaper config (source
   path / “managed by Easel” status).
3. **Hosting matrix on Plasma (updated):**

   | Content | Preferred host | Fallback |
   | --- | --- | --- |
   | Appearance light/dark | Built-in day/night package **or** Easel plugin | Still poller via `org.kde.image` |
   | Dense solar / h24 | **Easel plugin** (Rust schedule eval + `active.json` IPC) | Still poller via `org.kde.image` |
   | Live animated/video | **Easel plugin** (Stage 6) | Poster still |

4. **Apply path:** `PlasmaBackend` prefers setting `wallpaperPlugin` to the Easel plugin id
   when installed; otherwise retain ADR 0007 behavior (`org.kde.image`). Dense solar never
   requires zzag.
5. **Built-in day/night remains valuable** for users who want zero Easel process at idle for
   Appearance-only sets. The plugin does not replace that capability; it supersedes zzag as
   Easel’s preferred dense-solar host on Plasma.

## Consequences

- Scaffold lives at `apps/easel-plasma-wallpaper/` (KPackage + QML); desktop packaging
  installs it beside the app.
- Detection joins `plasma_dynamic_plugin_id()` / `easel_plasma_plugin_id()` probe.
- Still-frame IPC: desktop writes `{data}/plasma-wallpaper/active.json`; the Easel
  plugin polls it and updates `Image` without `evaluateScript` on every tick
  (Stage 6.3). Topology/plugin bind still uses a one-shot D-Bus script.
- Dense solar/h24 schedule evaluation stays in Rust (`due_dynamic_stills` /
  `active_frame_with_context`); `prefers_still_frame_host` skips zzag native apply
  (Stage 6.4).
- Docs must say: stock Plasma day/night ≠ Apple Dynamic Desktop; Easel plugin ≈ portable
  schedule host under Plasma’s wallpaper API.

## References

- `docs/adr/0007-plasma-dynamic-host-options.md`
- `docs/adr/0003-dynamic-and-live-wallpapers.md`
- https://develop.kde.org/docs/plasma/
- https://api.kde.org/qml-org-kde-plasma-plasmoid-wallpaperitem.html
- https://blog.vladzahorodnii.com/2025/08/11/dark-mode-improvements-in-plasma/
