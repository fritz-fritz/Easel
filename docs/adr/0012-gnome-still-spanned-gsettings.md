# ADR 0012: GNOME still wallpapers via spanned gsettings composite

- Status: accepted
- Date: 2026-08-08

## Context

Stage 7.2 needs a GNOME-family still backend. `org.gnome.desktop.background` exposes a
single `picture-uri` (and `picture-uri-dark`) with `picture-options` such as `zoom`,
`scaled`, and `spanned`. There is no stable public per-monitor still URI API that matches
Easel’s spanning crops (ADR 0011 deferred this until an honest contract existed).

## Decision

Ship `gnome-gsettings` as a still `WallpaperBackend` selected only when the session looks
GNOME-family **and** `gsettings get org.gnome.desktop.background picture-uri` succeeds.
Session hint is a `GNOME` token in `XDG_CURRENT_DESKTOP` (colon-separated), `gnome` in
`DESKTOP_SESSION`, or a live `org.gnome.Shell` on the session bus. A bare `ubuntu` desktop
token is **not** enough (non-GNOME Ubuntu flavors). Presence of `gsettings` alone is
insufficient (XFCE images also provide it).

Probe order remains: `plasma6` → `xfce-xfconf` → `gnome-gsettings` → `x11-feh`.

Apply behavior:

- One display: set `picture-uri` (+ dark) with `picture-options=zoom`.
- Multiple displays: composite Easel’s per-display crop PNGs onto a black virtual-desktop
  canvas sized to the axis-aligned bounding box of their `logical_rect`s, write a temp
  PNG, then set `picture-options=spanned` with that file URI.
- `VirtualDesktop` outputs use `spanned` directly.
- `NativeDynamic` remains unsupported (no public GNOME dynamic-HEIC host).

Capabilities report `per_display_images=true` because Easel still produces and places
per-display crops; the OS transport is one spanned image. Document that distinction in
the support matrix so operators are not told GNOME has independent per-monitor URIs.

## Consequences

- Multi-monitor spanning works on GNOME without undocumented Shell APIs.
- Stagger gaps outside any monitor remain black in the spanned canvas (same as the
  virtual desktop).
- Live wallpapers on GNOME remain unsupported (poster fallback through this still path).
- Revisit if GNOME publishes a stable per-monitor background API.

## References

- `docs/adr/0011-linux-still-wallpaper-backends.md`
- https://gitlab.gnome.org/GNOME/gsettings-desktop-schemas
