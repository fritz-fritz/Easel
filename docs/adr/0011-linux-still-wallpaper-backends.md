# ADR 0011: Linux still wallpaper backends (Stage 7.1)

- Status: accepted
- Date: 2026-08-08

## Context

Stage 6 completed Plasma still + live hosts. Non-Plasma Linux sessions (including the
Cursor Cloud XFCE desktop) still returned `NoBackend`, so Compose **Apply** could not
push stills. Architecture already names GNOME (`gsettings`), XFCE (`xfconf`), and a
custom-command / generic X adapter as candidates. Live motion on non-Plasma desktops
remains a later Stage 7 slice; this ADR covers **still** apply only.

## Decision

Probe Linux still backends in this order and pick the first that passes an explicit
capability check (never OS-name alone):

1. **KDE Plasma 6** (`plasma6`) — existing D-Bus / plugin path.
2. **XFCE** (`xfce-xfconf`) — `xfconf-query` on `xfce4-desktop` backdrop properties
   (`/backdrop/screen0/monitor<NAME>/workspaceN/last-image`). Match Easel crops to
   XRandR logical monitors by geometry; create missing monitor properties when needed.
3. **GNOME family** (`gnome-gsettings`) — Stage 7.2 / ADR 0012: session-hinted
   `gsettings` on `org.gnome.desktop.background`; multi-monitor as one spanned composite
   of per-display crops (no public per-monitor URI).
4. **Generic X11** (`x11-feh`) — `feh --no-fehbg --bg-fill` with one image per XRandR
   monitor in list order. Used only when no desktop-native channel is available
   (tiling WMs, minimal sessions). Do **not** prefer feh over XFCE/`xfdesktop` or GNOME,
   which own the backdrop and would fight a root-pixmap setter.
5. Otherwise `NoBackend`.

Capabilities reported today:

| Backend | per-display | virtual desktop | native dynamic | notes |
| --- | --- | --- | --- | --- |
| plasma6 | yes | no | yes (day/night / plugin) | Live host separate (`plasma6-live`). |
| xfce-xfconf | yes | yes (same path all monitors) | no | Workspaces exist in xfconf; not yet first-class in Easel. |
| gnome-gsettings | yes (spanned composite) | yes | no | ADR 0012; single `picture-uri` transport. |
| x11-feh | yes | yes (repeat path) | no | Session-persistent only while the root pixmap lasts / caller re-applies. |

## Consequences

- Cloud XFCE VMs can Apply stills and dynamic-still frames without installing Plasma.
- Support matrix (`docs/PLATFORM_SUPPORT.md`) must list each backend’s probe evidence.
- Live wallpapers on XFCE/GNOME/generic X remain unsupported (poster fallback via these
  still backends). A future Wayland/X live host is a separate decision.

## References

- `docs/ARCHITECTURE.md` (Platform backends)
- `docs/PLATFORM_SUPPORT.md`
- `docs/adr/0003-dynamic-and-live-wallpapers.md`
- `docs/adr/0012-gnome-still-spanned-gsettings.md`
- https://docs.xfce.org/xfce/xfdesktop/start
- https://feh.finalrewind.org/
