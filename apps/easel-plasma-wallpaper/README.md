# Easel Plasma wallpaper plugin

Plasma 6 wallpaper package (`KPackageStructure: Plasma/Wallpaper`) that presents
Easel as the OS wallpaper host. Library, schedules, and spanning stay in
`easel-desktop`; this package renders the active frame under plasmashell.

See [ADR 0008](../../docs/adr/0008-plasma-wallpaper-plugin-host.md).

## Install (development)

```sh
PLUGIN_ID=net.fritztech.easel.wallpaper
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/plasma/wallpapers/${PLUGIN_ID}"
mkdir -p "$DEST"
cp -a metadata.json contents "$DEST/"
# Restart plasmashell or log out/in, then choose Wallpaper type "Easel".
```

## Status

Scaffold only: shows the `Image` config key (same contract as `org.kde.image`).
Dense solar scheduling and live media IPC land with Stage 6 follow-ups.
