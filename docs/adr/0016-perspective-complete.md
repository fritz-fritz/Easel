# ADR 0016: Perspective complete (live UV + panel angles + wizard)

- Status: accepted
- Date: 2026-08-08

## Context

Stage 7.5 (ADR 0015) shipped distance-first coplanar `ViewerPose` and Compose
Physical calibration controls. Still Apply / posters / slideshow used projective
raster; continuous Plasma live stayed on axis-aligned UV crops. Per-display
tilt/yaw and a dedicated calibration experience were deferred.

## Decision

1. **Projective live UV:** `LiveCompositorFrame` plans the same
   `AngularPerspective` maps as posters. Plasma `active.json` schema **v3**
   carries optional per-display perspective uniforms; the wallpaper plugin
   samples with a precompiled `ShaderEffect` (`.qsb`) when present, otherwise
   keeps the AA GIF/`sourceRect` path.

2. **Per-display tilt/yaw:** `Display.tilt_deg` / `Display.yaw_deg` (±45°,
   identity `0`) tip each panel off the coplanar wall. Arrangement schema **v3**
   migrates v1/v2. Angles apply only while the global viewer pose is active.
   Math lifts content UV onto a tilted plane, then projects through the eye;
   the image map remains on the z=0 span from `place_source_on_span`.

3. **Calibration wizard:** Compose opens a modal multi-step dialog (enable →
   distance/eye → selected-display tilt/yaw) in addition to the Physical
   calibration GroupBox numeric controls.

## Consequences

- Live and poster crops stay aligned when perspective is on.
- Plasma plugin requires the shipped `perspective.frag.qsb` asset.
- Rematch preserves panel angles with other user calibration.

## References

- `docs/adr/0015-perspective-viewer-correction.md`
- `docs/adr/0008-plasma-wallpaper-plugin-host.md`
- `docs/ARCHITECTURE.md` (physical layout space)
