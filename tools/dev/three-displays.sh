#!/usr/bin/env bash
# Configure the interactive dev X server (the VNC / XFCE desktop on DISPLAY=:1)
# to expose THREE logical monitors via RandR 1.5 `--setmonitor`, so the Easel
# desktop app enumerates three displays when it probes `Qt.application.screens`.
#
# This mirrors CI's three-monitor intent: CI validates multi-display rendering
# against the `DP-1`/`DP-2`/`DP-3` fixture in `apps/easel-desktop/src/fixtures.rs`
# (staggered landscape monitors with distinct physical sizes). Here we recreate
# that layout on the *live* X server so the running GUI reports three displays.
#
# Pixel resolutions are scaled down to fit the 1920x1200 VNC framebuffer, but the
# connector names and physical millimeter sizes match the CI fixture so the
# physical-continuity math exercises the same shape of input.
#
# Usage:
#   tools/dev/three-displays.sh          # define the 3 monitors on $DISPLAY (default :1)
#   tools/dev/three-displays.sh reset    # remove them and restore the single auto monitor
#   DISPLAY=:1 tools/dev/three-displays.sh
#
# This script is intentionally defensive: it never exits non-zero, so it is safe
# to wire into `.cursor/environment.json` `start` without risking env startup.

set -u

DISPLAY="${DISPLAY:-:1}"
export DISPLAY

log() { printf '[three-displays] %s\n' "$*"; }

if ! command -v xrandr >/dev/null 2>&1; then
  log "xrandr not found; skipping (install x11-xserver-utils to enable)."
  exit 0
fi

# Wait (briefly) for the X server to accept connections. The VNC/XFCE session is
# started by the VM image and may not be ready the instant the environment boots.
ready=""
for _ in $(seq 1 30); do
  if xrandr --query >/dev/null 2>&1; then
    ready="yes"
    break
  fi
  sleep 1
done

if [ -z "$ready" ]; then
  log "X server on DISPLAY=$DISPLAY not reachable; skipping."
  exit 0
fi

# The physical output backing the framebuffer (e.g. VNC-0 on TigerVNC).
OUTPUT="$(xrandr --query 2>/dev/null | awk '/ connected/{print $1; exit}')"
if [ -z "$OUTPUT" ]; then
  log "no connected output found; skipping."
  exit 0
fi

# Remove any monitors we previously defined (ignore errors if absent).
for name in DP-1 DP-2 DP-3; do
  xrandr --delmonitor "$name" >/dev/null 2>&1 || true
done

if [ "${1:-}" = "reset" ]; then
  log "reset: removed DP-1/DP-2/DP-3; live output '$OUTPUT' restored."
  xrandr --listmonitors 2>/dev/null || true
  exit 0
fi

# Three staggered landscape monitors. Geometry is "Wpx/Wmm x Hpx/Hmm + Xpx + Ypx".
# Names + millimeter sizes match the CI fixture (fixtures.rs). Total width 1920px.
#   DP-1: medium, left,   raised baseline
#   DP-2: largest, center, top
#   DP-3: smallest, right, raised baseline
xrandr --setmonitor DP-1 640/600x360/340+0+180    "$OUTPUT" >/dev/null 2>&1 || true
xrandr --setmonitor DP-2 768/700x432/400+640+0    none      >/dev/null 2>&1 || true
xrandr --setmonitor DP-3 512/530x288/300+1408+180 none      >/dev/null 2>&1 || true

log "configured 3 monitors on DISPLAY=$DISPLAY (output '$OUTPUT'):"
xrandr --listmonitors 2>/dev/null || true
exit 0
