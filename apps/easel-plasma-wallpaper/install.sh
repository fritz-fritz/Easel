#!/usr/bin/env bash
# Install the Easel Plasma wallpaper plugin for the current user (ADR 0008).
set -euo pipefail

PLUGIN_ID="net.fritztech.easel.wallpaper"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/plasma/wallpapers/${PLUGIN_ID}"
QSB_SRC="${SCRIPT_DIR}/contents/ui/shaders/perspective.frag.qsb"

if [[ ! -s "$QSB_SRC" ]]; then
  echo "error: missing projective live shader pack: ${QSB_SRC}" >&2
  echo "Rebuild with: tools/dev/compile-plasma-shaders.sh" >&2
  exit 1
fi

mkdir -p "$DEST"
# Refresh package contents (including dotfiles) without deleting DEST itself.
find "$DEST" -mindepth 1 -delete
cp -a "$SCRIPT_DIR/metadata.json" "$DEST/"
cp -a "$SCRIPT_DIR/contents" "$DEST/"

QSB_DEST="${DEST}/contents/ui/shaders/perspective.frag.qsb"
if [[ ! -s "$QSB_DEST" ]]; then
  echo "error: install incomplete — perspective.frag.qsb missing under ${DEST}" >&2
  exit 1
fi

echo "Installed ${PLUGIN_ID} → ${DEST}"
echo "Verified projective live shader ($(wc -c <"${QSB_DEST}") bytes)"
echo "Restart plasmashell (or log out/in), then choose Wallpaper type \"Easel\"."
echo "Still-frame Apply from easel-desktop will prefer this plugin when detected."
