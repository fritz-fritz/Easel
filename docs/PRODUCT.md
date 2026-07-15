# Product plan

## Vision

Wallspan should make sophisticated multi-monitor wallpaper layouts feel as approachable as
choosing a single desktop background. Advanced correction remains available without forcing
every user through a calibration workflow.

## Product principles

- **Modern by default:** visual browsing, live previews, direct manipulation, accessible
  controls, system theme integration, and clear background-task state.
- **Correct across displays:** physical continuity matters more than treating the desktop as
  one simplistic pixel rectangle.
- **Motion with restraint:** animated media is silent, power-aware, synchronized across displays,
  and always has a static fallback.
- **Local-first:** all core wallpaper and scheduling features work without an account or
  network connection.
- **Provider-respectful:** source terms, attribution, license information, and usage reporting
  are part of the data model rather than UI footnotes.
- **Honest capabilities:** unsupported behavior is reported per desktop backend instead of
  being silently approximated.
- **Cross-platform core, native edges:** share the renderer and product behavior while using
  the correct integration mechanism for each operating system and desktop environment.

## Primary experiences

### Setup and calibration

1. Discover connected displays.
2. Match each output to a stable identity.
3. Confirm physical size and rotation.
4. Arrange displays in physical space with snapping and numeric fine-tuning.
5. Optionally enter bezel widths and viewing perspective.
6. Save arrangements independently from wallpaper profiles.

### Wallpaper composition

- Select a local still image, animated image, or video, or an online still image.
- Choose all displays, one display, or an arbitrary display group.
- Select cover, contain, crop, focal point, zoom, and alignment behavior.
- Preview native per-display output and bezel discontinuities.
- For live media, preview motion, choose loop and frame-rate policy, and show current backend
  support before applying.
- Apply without blocking the interface.

### Discovery and library

- Search local folders and approved online catalogs.
- Filter by orientation, aspect ratio, minimum native resolution, color, license, creator,
  source, and content rating.
- Save assets to collections and retain immutable provenance.
- Show creator and license information beside every remote result.
- Exclude assets that cannot satisfy the target display group's required pixel dimensions.

### Automation

- Fixed interval and wall-clock schedules.
- Separate schedules by display group, profile, activity, workspace, or virtual desktop where
  supported.
- Sunrise/sunset and time-of-day rules.
- Pause, skip, history, and avoid-repeat controls.
- React to display topology changes without corrupting the saved physical arrangement.

### Dynamic and live wallpaper

- Build dynamic-still sets driven by wall-clock time, sunrise/sunset, or ordinary schedules.
- Import local animated image and video files and extract a configurable poster frame.
- Preserve one logical playback clock across every crop in a spanning display group.
- Loop or stop at the final frame; live media never emits audio.
- Pause decoding on battery, session lock, sleep, or while a full-screen application is active,
  according to profile policy and backend capability.
- Degrade visibly to the poster frame when a codec or desktop live host is unavailable.
- Report static, dynamic-still, animated-image, and video support separately for every active
  platform backend.

## Initial non-goals

- Accounts or cloud profile synchronization.
- A hosted proxy for third-party credentials.
- Streaming video URLs or audio playback.
- Online motion catalogs until a provider explicitly authorizes the wallpaper use case.
- A plugin ABI for untrusted native code.
- Scraping image websites that do not provide an authorized API.
- Reproducing the interface or internal organization of the reference application.

## Release definition

The first production release requires:

- Linux Plasma 6/Wayland and Windows support.
- Local folders plus at least one legally suitable online catalog.
- Simple span, per-display, groups, PPI, bezel, focal point, and zoom.
- Stable monitor identity with topology-change recovery.
- Profiles, interval scheduling, tray controls, CLI, and diagnostic export.
- Keyboard navigation, screen-reader labels, high-DPI behavior, and system light/dark themes.
- Crash-safe configuration writes and cache cleanup.

macOS and perspective correction may land before or after 1.0 depending on validation capacity.
Dynamic stills are part of the production direction. Animated-image and video support may remain
an experimental, backend-specific feature until the live host meets the quality and power budgets.
