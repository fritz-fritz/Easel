# ADR 0015: Perspective / viewer correction (distance-first)

- Status: accepted
- Date: 2026-08-08

## Context

Stage 2 delivered axis-aligned physical spanning (PPI + bezel → mm content rects →
pixel crops). ARCHITECTURE names view distance and monitor angle as part of physical
layout space and asks the renderer for projective transforms. PRODUCT treats viewing
perspective as an optional calibration step that must not be mandatory.

Per-display tilt/yaw is a larger UX and math surface. A first Stage 7.5 slice should prove
optional projective correction with a global viewer pose and Compose controls, while keeping
today’s PhysicalSpan bit-identical when correction is off.

## Decision

1. **Global `ViewerPose` on `DisplayArrangement`** (schema v2), not on `Profile`. Fields:
   `enabled`, `view_distance_mm`, `eye_offset_x_mm`, `eye_offset_y_mm` (offsets relative to
   `content_bounds` center).

2. **Identity default:** `enabled == false` (or non-finite / non-positive distance) keeps the
   existing axis-aligned `physical_operations` path unchanged.

3. **Distance-first math:** panels remain coplanar. Correction maps source through the
   viewer’s angular FOV (`atan2` against view distance) so finite distance produces a
   non-linear warp; as distance → ∞ the mapping approaches the AA PhysicalSpan result.
   Per-display monitor angles are deferred.

4. **Still Apply / posters / still-slideshow** use the projective raster path when enabled.
   Continuous Plasma live keeps AA UV crops for this slice; corrected stills still cover
   poster fallback and slideshow frames.

5. **Compose UI:** extend the existing Physical calibration GroupBox (checkbox + distance +
   eye offsets). No dedicated wizard in Stage 7.5.

## Consequences

- Arrangement TOML grows a `viewer` table; v1 documents migrate with pose disabled.
- `RENDERER_VERSION` / arrangement cache tokens include pose fields.
- Follow-up: per-display tilt/yaw, dedicated calibration experience, projective live UV.

## References

- `docs/ARCHITECTURE.md` (physical layout space, projective transforms)
- `docs/PRODUCT.md` (optional viewing perspective)
- `docs/ROADMAP.md` (Stage 7 perspective / calibration)
