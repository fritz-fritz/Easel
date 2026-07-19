# ADR 0006: Apple HEIC is the dynamic-still interchange; native per-display apply

- Status: accepted
- Date: 2026-07-18

## Context

Stage 5 initially modeled dynamic stills as sparse time-of-day keys (morning / noon /
evening) applied through Easel's still `WallpaperBackend` poller. That understates the
capability users expect:

- **macOS** ships Dynamic Desktop as a multi-image HEIC whose first image carries XMP
  `apple_desktop:{solar,apr,h24}` metadata — a base64 binary plist describing dense solar
  altitude/azimuth samples, appearance (light/dark) indices, or 24-hour time points. The OS
  evaluates the schedule natively.
- **KDE Plasma** has first-class dynamic wallpaper support (community
  `plasma5-wallpapers-dynamic`, and Plasma 6 day/night packages via KNightTime) that also
  understands solar elevation/azimuth HEIC/AVIF packages. Apple HEIC is not bit-compatible
  but is the common source community converters start from.
- **Windows** has no public dynamic-HEIC wallpaper API. `IDesktopWallpaper` applies still
  files only; third-party tools (e.g. WinDynamicDesktop) extract frames and schedule them.

Multi-monitor physical composition means a single vendor HEIC cannot be handed to the OS
unchanged: each display needs its own crop of every frame, then a bundle the platform can
evaluate.

## Decision

1. **Canonical interchange is Apple-style dynamic HEIC.** Easel imports `solar`, `apr`
   (appearance), and `h24` (time) metadata into the domain `DynamicStillSet`. Original bytes
   and decoded frame assets are retained for provenance and re-export.

2. **Domain keys match the HEIC models, not a three-slot approximation.**
   - `SolarPosition { altitude_deg, azimuth_deg }` — nearest-neighbor to the computed sun;
   - `TimeOfDay` — dense wall-clock samples (h24);
   - `Appearance { Light | Dark }` — appearance-mode sets;
   - Legacy `Solar { Sunrise|Sunset, offset }` remains for authored schedules.
   Each still set records a `schedule_kind` so evaluation uses the correct rule.

3. **Apply path prefers native dynamic bundles when the backend can host them.**
   Pipeline: deconstruct HEIC → plan per-display crops for every frame → encode one native
   dynamic package per display (macOS HEIC, Plasma HEIC/AVIF) → hand packages to the OS.
   Where the backend cannot host a dynamic package (**Windows** today), Easel falls back to
   the Stage 5 still poller: evaluate the domain set and apply the active cropped still.

4. **`BackendCapabilities` grows `native_dynamic_bundle`.** Plasma and future macOS backends
   may report true once encode/apply is wired; Windows reports false and keeps still apply.
   Cross-fade remains a separate capability.

5. **Easel remains the portable source of truth.** Even when the OS evaluates a native
   package, Compose authoring, library provenance, physical layout, and CLI status operate on
   `DynamicStillSet`. Native packages are derived outputs, like raster cache files.

## Consequences

- Importing a Mojave-class HEIC yields ~16 solar-position frames, not three ToD slots.
- Multi-monitor spanning requires re-encoding N display bundles × M frames — expensive but
  correct; pre-render/cache keys include arrangement geometry and renderer version.
- Windows users still get dynamic behavior via Easel's poller; they do not get a native HEIC
  host until Microsoft exposes one.
- Plasma hosting is split (ADR 0007): built-in day/night packages for Appearance sets;
  community zzag HEIC for dense solar when installed; still poller otherwise.
- Encode + per-display crop cache keys include arrangement geometry and renderer version.

## References

- https://nshipster.com/macos-dynamic-desktop/
- https://github.com/zzag/plasma5-wallpapers-dynamic
- https://github.com/bcyran/timewall
- `docs/adr/0003-dynamic-and-live-wallpapers.md`
- `docs/adr/0005-dynamic-stills.md`
- `docs/LIVE_WALLPAPERS.md`
