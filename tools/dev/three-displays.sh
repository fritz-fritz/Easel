#!/usr/bin/env bash
# Configure the interactive dev X server (the VNC / XFCE desktop on DISPLAY=:1)
# to expose THREE logical monitors via RandR 1.5 `--setmonitor`, so the Easel
# desktop app enumerates three displays when it probes `Qt.application.screens`.
#
# Layout (fixture-like stagger, full consumer resolutions):
#   DP-1  left    4K      3840×2160
#   DP-2  center  UWQHD   3440×1440
#   DP-3  right   1080p   1920×1080
#
# Virtual-screen model (important for Cloud VNC):
# - The three monitors are different sizes and staggered (not full-height strips).
# - Their axis-aligned bounding box is 9200×2360. We switch the VNC-0 output to a
#   matching mode so the X/VNC framebuffer *is* that virtual desktop — VNC shows
#   the screen that contains the three monitors (black only in stagger gaps).
# - Every monitor is bound to the live output (`VNC-0`), never RandR `none`.
#   Otherwise XFCE writes xfconf successfully but never paints those regions.
# - noVNC will need zoom/pan to navigate a 9200×2360 desktop; that is expected.
#
# WARNING: Opt-in only. Cloud `start` runs `reset` so interactive work defaults
# to one full 1920×1200 monitor. Enable the split while exercising Easel
# multi-display behavior, then reset.
#
# Usage:
#   tools/dev/three-displays.sh          # define the 3 monitors on $DISPLAY (default :1)
#   tools/dev/three-displays.sh reset    # remove them and restore 1920×1200
#   DISPLAY=:1 tools/dev/three-displays.sh
#
# This script is intentionally defensive: it never exits non-zero, so it is safe
# to wire into `.cursor/environment.json` `start` (as `reset`) without risking
# env startup.

set -u

DISPLAY="${DISPLAY:-:1}"
export DISPLAY

log() { printf '[three-displays] %s\n' "$*"; }

# Bounding box of the staggered full-res layout below.
VIRTUAL_W=9200
VIRTUAL_H=2360
VIRTUAL_MODE="${VIRTUAL_W}x${VIRTUAL_H}_60.00"
DEFAULT_MODE="1920x1200"

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
  # After a failed resize the output can briefly show as disconnected but still
  # list modes — fall back to the preferred TigerVNC name.
  if xrandr --query 2>/dev/null | grep -q '^VNC-0 '; then
    OUTPUT="VNC-0"
  else
    log "no connected output found; skipping."
    exit 0
  fi
fi

virtual_modeline() {
  # Prefer cvt when present; fall back to a known-good CVT line for 9200×2360.
  if command -v cvt >/dev/null 2>&1; then
    cvt "$VIRTUAL_W" "$VIRTUAL_H" 60 | sed -n 's/^Modeline "[^"]*" //p'
  else
    printf '%s\n' "1872.00  9200 9968 10984 12768  2360 2363 2373 2444 -hsync +vsync"
  fi
}

ensure_virtual_mode() {
  if ! xrandr --query 2>/dev/null | grep -q "$VIRTUAL_MODE"; then
    local modeline
    modeline="$(virtual_modeline)"
    # shellcheck disable=SC2086
    xrandr --newmode "$VIRTUAL_MODE" $modeline >/dev/null 2>&1 || true
  fi
  xrandr --addmode "$OUTPUT" "$VIRTUAL_MODE" >/dev/null 2>&1 || true
}

restart_xfce_shell() {
  pkill -x xfwm4 >/dev/null 2>&1 || true
  pkill -x xfdesktop >/dev/null 2>&1 || true
  pkill -x xfce4-panel >/dev/null 2>&1 || true
  sleep 1
  nohup xfwm4 >/dev/null 2>&1 &
  sleep 1
  nohup xfdesktop >/dev/null 2>&1 &
  nohup xfce4-panel >/dev/null 2>&1 &
}

# Remove any monitors we previously defined (ignore errors if absent).
for name in DP-1 DP-2 DP-3; do
  xrandr --delmonitor "$name" >/dev/null 2>&1 || true
done

if [ "${1:-}" = "reset" ]; then
  log "reset: removed DP-1/DP-2/DP-3; restoring ${DEFAULT_MODE} on '$OUTPUT'."
  # Prefer the stock mode so the Cloud VNC view returns to a usable desktop.
  if xrandr --query 2>/dev/null | grep -q "$DEFAULT_MODE"; then
    xrandr --output "$OUTPUT" --mode "$DEFAULT_MODE" >/dev/null 2>&1 || \
      xrandr --fb "$DEFAULT_MODE" >/dev/null 2>&1 || true
  else
    xrandr --fb "$DEFAULT_MODE" >/dev/null 2>&1 || true
    xrandr --output "$OUTPUT" --auto >/dev/null 2>&1 || true
  fi
  xrandr --listmonitors 2>/dev/null || true
  # XFCE/xfwm can keep a stale _NET_DESKTOP_GEOMETRY after mode changes.
  fb_geom="$(xrandr --current 2>/dev/null | awk '/\*/{print $1; exit}' | tr -d '+*')"
  desk_geom="$(xprop -root _NET_DESKTOP_GEOMETRY 2>/dev/null | awk -F'= ' '{print $2}' | tr -d ' ')"
  fb_h="${fb_geom##*x}"
  desk_h="${desk_geom##*,}"
  if [ -n "${fb_h:-}" ] && [ -n "${desk_h:-}" ] && [ "$fb_h" != "$desk_h" ]; then
    log "stale desktop geometry height ${desk_h} (framebuffer ${fb_h}); restarting xfce shell"
    restart_xfce_shell
  else
    pkill -x xfdesktop >/dev/null 2>&1 || true
    sleep 1
    nohup xfdesktop >/dev/null 2>&1 &
  fi
  exit 0
fi

ensure_virtual_mode

# Resize the VNC framebuffer to the virtual desktop *before* carving monitors.
# Changing --fb while the output mode disagrees can disconnect VNC-0.
if ! xrandr --output "$OUTPUT" --mode "$VIRTUAL_MODE" >/dev/null 2>&1; then
  log "could not switch $OUTPUT to $VIRTUAL_MODE; leaving single-monitor session."
  xrandr --listmonitors 2>/dev/null || true
  exit 0
fi

# Three staggered landscape monitors at full consumer resolutions.
# Geometry is "Wpx/Wmm x Hpx/Hmm + Xpx + Ypx". Bounding box ${VIRTUAL_W}×${VIRTUAL_H}.
#   DP-1 left   4K     3840×2160  (~27", 600×340 mm)
#   DP-2 center UWQHD  3440×1440  (~34" ultrawide, 800×335 mm)
#   DP-3 right  1080p  1920×1080  (~22", 480×270 mm)
# Center sits at the top; sides drop slightly (fixture-like stagger).
xrandr --setmonitor DP-1 3840/600x2160/340+0+200       "$OUTPUT" >/dev/null 2>&1 || true
xrandr --setmonitor DP-2 3440/800x1440/335+3840+0      "$OUTPUT" >/dev/null 2>&1 || true
xrandr --setmonitor DP-3 1920/480x1080/270+7280+400    "$OUTPUT" >/dev/null 2>&1 || true

# xfdesktop caches monitor lists; nudge so per-monitor backdrops refresh.
pkill -x xfdesktop >/dev/null 2>&1 || true
sleep 1
nohup xfdesktop >/dev/null 2>&1 &

log "configured 3 monitors on DISPLAY=$DISPLAY (virtual screen ${VIRTUAL_W}x${VIRTUAL_H}, output '$OUTPUT'):"
xrandr --listmonitors 2>/dev/null || true
exit 0
