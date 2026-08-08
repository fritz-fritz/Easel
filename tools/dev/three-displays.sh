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
# WARNING: This is an *opt-in* multi-display test layout. It does not enlarge the
# VNC framebuffer; it subdivides it. The staggered monitors leave large black
# regions in the VNC view and shrink the usable XFCE panel/desktop. Cloud `start`
# therefore runs `reset` so interactive work defaults to one full 1920x1200 monitor.
# Enable the split only while exercising Easel multi-display behavior, then reset.
#
# Usage:
#   tools/dev/three-displays.sh          # define the 3 monitors on $DISPLAY (default :1)
#   tools/dev/three-displays.sh reset    # remove them and restore the single auto monitor
#   DISPLAY=:1 tools/dev/three-displays.sh
#
# This script is intentionally defensive: it never exits non-zero, so it is safe
# to wire into `.cursor/environment.json` `start` (as `reset`) without risking
# env startup.

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
  # After deleting RandR monitors, XFCE/xfwm can keep a stale tiny
  # _NET_DESKTOP_GEOMETRY (e.g. 640x540 from the staggered split), which leaves
  # most of the VNC viewport black. Restart the shell when geometry disagrees
  # with the live framebuffer so interactive work/recordings stay usable.
  fb_w="$(xrandr --current 2>/dev/null | awk '/\*/{print $1; exit}' | cut -dx -f1)"
  desk_w="$(xprop -root _NET_DESKTOP_GEOMETRY 2>/dev/null | awk -F'= ' '{print $2}' | cut -d, -f1 | tr -d ' ')"
  if [ -n "${fb_w:-}" ] && [ -n "${desk_w:-}" ] && [ "$fb_w" != "$desk_w" ]; then
    log "stale desktop geometry ${desk_w}px wide (framebuffer ${fb_w}px); restarting xfce shell"
    pkill -x xfwm4 >/dev/null 2>&1 || true
    pkill -x xfdesktop >/dev/null 2>&1 || true
    pkill -x xfce4-panel >/dev/null 2>&1 || true
    sleep 1
    nohup xfwm4 >/dev/null 2>&1 &
    sleep 1
    nohup xfdesktop >/dev/null 2>&1 &
    nohup xfce4-panel >/dev/null 2>&1 &
  fi
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
