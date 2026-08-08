#!/usr/bin/env bash
# Rebuild precompiled Qt Shader Baker packs for the Easel Plasma wallpaper plugin.
# Requires Qt 6 `qsb` (Debian/Ubuntu: qt6-shadertools-dev → /usr/lib/qt6/bin/qsb).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
SHADER_DIR="${REPO_ROOT}/apps/easel-plasma-wallpaper/contents/ui/shaders"
SRC="${SHADER_DIR}/perspective.frag"
OUT="${SHADER_DIR}/perspective.frag.qsb"

if [[ ! -f "$SRC" ]]; then
  echo "error: missing fragment source: ${SRC}" >&2
  exit 1
fi

QSB_BIN="${QSB:-}"
if [[ -z "$QSB_BIN" ]]; then
  if command -v qsb >/dev/null 2>&1; then
    QSB_BIN="$(command -v qsb)"
  elif [[ -x /usr/lib/qt6/bin/qsb ]]; then
    QSB_BIN=/usr/lib/qt6/bin/qsb
  else
    echo "error: qsb not found (install qt6-shadertools / set QSB=)" >&2
    exit 1
  fi
fi

# Match the checked-in pack targets (GLSL ES/desktop + HLSL + MSL + SPIR-V).
"${QSB_BIN}" \
  --glsl "100 es,120,150" \
  --hlsl 50 \
  --msl 12 \
  -o "${OUT}" \
  "${SRC}"

if [[ ! -s "$OUT" ]]; then
  echo "error: qsb produced empty output: ${OUT}" >&2
  exit 1
fi

echo "Wrote ${OUT} ($(wc -c <"${OUT}") bytes) via ${QSB_BIN}"
