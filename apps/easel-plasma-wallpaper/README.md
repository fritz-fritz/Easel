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
wallpaper roots and prefers it for still-frame apply.

## Still-frame IPC

Desktop automation writes `{data}/plasma-wallpaper/active.json` after each
still apply. The plugin polls that file (~750ms) and picks the entry whose
geometry matches this containment’s screen. After the first bind (plugin +
`StateFile` + seed `Image` via D-Bus), subsequent dense-solar ticks only update
the JSON — no `PlasmaShell.evaluateScript` until display topology changes.

Appearance day/night packages still use built-in `org.kde.image` + KNightTime.
Dense solar HEIC packages still use zzag when present; still-poller frames use
this IPC path when the Easel plugin is installed.

## Status

Stage 6.3: still-image host with `Image` + `StateFile` IPC. Live media and
in-plugin schedule evaluation remain follow-ups.
