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
wallpaper roots and prefers it for still-frame apply. Appearance day/night
packages still use built-in `org.kde.image` + KNightTime; dense solar HEIC still
uses zzag when present until schedule IPC lands.

## Status

Stage 6.1: still-image host with `Image` config (same contract as `org.kde.image`).
Dense solar evaluation and live media IPC are Stage 6 follow-ups.
