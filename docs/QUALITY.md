# Quality strategy

## Test layers

- **Domain tests:** schema validation, migration fixtures, provider policy, schedules, and stable
  identity matching.
- **Planner tests:** generated monitor layouts and exact per-output operation plans.
- **Raster tests:** small project-owned source images with tolerance-based comparisons.
- **Media tests:** project-owned short clips and animations covering timestamps, loop boundaries,
  poster extraction, missing codecs, and corrupt containers.
- **Provider contract tests:** recorded, sanitized responses plus limited live smoke tests.
- **Backend tests:** command/D-Bus/API construction without mutating the developer's wallpaper.
- **Live-host tests:** lifecycle and surface ownership with synthetic frames before real playback.
- **UI tests:** QML component behavior, keyboard navigation, model updates, and screenshot checks.
- **Manual matrix:** real fractional scaling, HDR mode, rotation, hotplug, sleep/wake, and desktop
  session combinations, plus codec and hardware-decode availability.

## Required layout fixtures

- One display.
- Two equal displays in a row.
- Negative logical coordinates.
- Vertical stack and T-shaped layout.
- Portrait plus landscape displays.
- Mixed scale factors including 125%, 150%, and 200%.
- Different physical sizes at the same resolution.
- Same physical size at different resolutions.
- Bezel corrections on internal and outer edges.
- Disconnected and reconnected display with a changed connector name.

## Performance budgets

- UI input remains responsive during discovery and rendering.
- Preview requests are cancelable and coalesced.
- A 12K source image never requires multiple full-size intermediate copies without an explicit
  reason.
- Thumbnail decoding is bounded and isolated from final-quality rendering.
- Completed output is atomic; canceled work does not replace the active wallpaper.
- Live playback defaults to a 30 fps ceiling and remains synchronized across a display group.
- Drift between display crops stays below one presented frame at the configured frame-rate ceiling.
- No source audio reaches an output device or initializes an audio session.
- Pausing for battery/full-screen policy stops decode work within two seconds.
- Sleep, lock, desktop restart, decoder failure, or host loss restores or retains a poster frame.

Live-media CPU, GPU, memory, and battery budgets require measurements on representative hardware
before a backend can graduate from experimental. Hardware decoding is preferred but does not
exempt a backend from the same measurements.

## CI policy

The default workspace excludes the Qt application so core checks run on all hosted platforms.
A dedicated Linux job installs Qt and checks the desktop crate. Windows and macOS Qt jobs are
added with their packaging stages rather than being represented as passing prematurely.

Media CI adds only short, project-owned, silent fixtures. Codec assertions are capability-aware so
the suite distinguishes an unavailable runtime decoder from an incorrect compositor result.
