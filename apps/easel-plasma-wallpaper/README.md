# Easel Plasma wallpaper plugin

Plasma 6 wallpaper package (`KPackageStructure: Plasma/Wallpaper`) that presents
Easel as the OS wallpaper host. Library, schedules, and spanning stay in
`easel-desktop`; this package renders the active frame under plasmashell.

See [ADR 0008](../../docs/adr/0008-plasma-wallpaper-plugin-host.md).

## Install (development)

```sh
./apps/easel-plasma-wallpaper/install.sh
# Restart plasmashell or log out/in, then choose Wallpaper type "Easel".
```

`PlasmaBackend` detects `net.fritztech.easel.wallpaper` under the usual Plasma
wallpaper roots and prefers it for still-frame apply. `PlasmaLiveBackend` is
selected for live media once this package is installed.

## Still-frame IPC

Desktop automation writes `{data}/plasma-wallpaper/active.json` after each
still apply. The plugin polls that file and picks the entry whose geometry
matches this containment’s screen. After the first bind (plugin + `StateFile` +
seed `Image` via D-Bus), subsequent dense-solar ticks only update the JSON —
no `PlasmaShell.evaluateScript` until display topology changes.

## Live media IPC

When Compose Apply starts a live session, `active.json` uses `mode: "live"` with:

- shared `media_time_ms` / `paused` / `pause_reason` from the desktop
  `PlaybackClock` + policy sensors;
- per-display `source_uv` crops from `plan_live_crops`;
- still `displays[].image` posters for startup and decoder failure.

The plugin plays muted GIF (`AnimatedImage`) or video (`MediaPlayer` +
`VideoOutput.sourceRect`) and resyncs to the shared clock.

## Dynamic stills

- **Appearance** light/dark: built-in `org.kde.image` + KNightTime day/night packages.
- **Dense solar / h24:** Rust evaluates `DynamicStillSet` and publishes cropped stills
  through this IPC path. No community zzag (or other external) wallpaper plugin is
  required.

## Status

Stage 6 complete for Plasma: still host + live playback IPC. Windows/macOS live
hosts remain unsupported (ADR 0010).
