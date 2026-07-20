#!/usr/bin/env bash
# Install the Easel Plasma wallpaper plugin for the current user (ADR 0008).
set -euo pipefail

PLUGIN_ID="net.fritztech.easel.wallpaper"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/plasma/wallpapers/${PLUGIN_ID}"

mkdir -p "$DEST"
# Refresh package contents (including dotfiles) without deleting DEST itself.
find "$DEST" -mindepth 1 -delete
cp -a "$SCRIPT_DIR/metadata.json" "$DEST/"
cp -a "$SCRIPT_DIR/contents" "$DEST/"

echo "Installed ${PLUGIN_ID} → ${DEST}"
echo "Restart plasmashell (or log out/in), then choose Wallpaper type \"Easel\"."
echo "Still-frame Apply from easel-desktop will prefer this plugin when detected."
