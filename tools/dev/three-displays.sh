#!/usr/bin/env bash
# Configure the interactive dev X server (the VNC / XFCE desktop on DISPLAY=:1)
# to expose THREE logical monitors via RandR 1.5 `--setmonitor`, so the Easel
# desktop app enumerates three displays when it probes `Qt.application.screens`.
#
# This mirrors CI's three-monitor intent: CI validates multi-display rendering
# against the `DP-1`/`DP-2`/`DP-3` fixture in `apps/easel-desktop/src/fixtures.rs`
# (distinct physical sizes). Here we recreate that *identity* on the live X
# server so the running GUI reports three displays.
#
# Layout notes (Cloud VNC is one 1920x1200 framebuffer / output `VNC-0`):
# - All three monitors are bound to the live output (not `none`). Monitors with
#   an empty output list are invisible to XFCE wallpaper painting, so Apply
#   appeared to succeed (xfconf wrote) while only one strip ever updated.
# - Pixel boxes are full-bleed vertical strips filling the framebuffer so every
#   VNC pixel belongs to a monitor (usable for demos/recordings). Connector
#   names and physical millimeter sizes still match the CI fixture; pixel
#   heights are stretched to 1200 so the staggered CI aspect ratios are not
#   preserved on this surface.
#
# WARNING: Opt-in only. Cloud `start` runs `reset` so interactive work defaults
# to one full 1920x1200 monitor. Enable the split while exercising Easel
# multi-display behavior, then reset.
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

# Three full-bleed vertical strips on the live output.
# Geometry is "Wpx/Wmm x Hpx/Hmm + Xpx + Ypx".
# Names + millimeter sizes match the CI fixture (fixtures.rs). Total 1920x1200.
#   DP-1: medium width, left
#   DP-2: largest width, center
#   DP-3: smallest width, right
xrandr --setmonitor DP-1 640/600x1200/340+0+0     "$OUTPUT" >/dev/null 2>&1 || true
xrandr --setmonitor DP-2 768/700x1200/400+640+0   "$OUTPUT" >/dev/null 2>&1 || true
xrandr --setmonitor DP-3 512/530x1200/300+1408+0  "$OUTPUT" >/dev/null 2>&1 || true

# xfdesktop caches monitor lists; nudge so per-monitor backdrops refresh.
if pgrep -x xfdesktop >/dev/null 2>&1; then
  pkill -x xfdesktop >/dev/null 2>&1 || true
  sleep 1
  nohup xfdesktop >/dev/null 2>&1 &
fi

log "configured 3 monitors on DISPLAY=$DISPLAY (output '$OUTPUT'):"
xrandr --listmonitors 2>/dev/null || true
exit 0
