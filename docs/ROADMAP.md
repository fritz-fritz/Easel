# Greenfield delivery roadmap

This is a capability plan, not a migration from another application. Each stage leaves the new
architecture internally coherent and testable.

## Stage 0 — Foundations

- Finalize product name, reverse-DNS application ID, repository ownership, and distribution
  license.
- Establish Rust/Qt CI, formatting, dependency review, and release signing decisions.
- Finish domain schemas and error taxonomy.
- Create generated geometry and image fixtures owned by this project.
- Record desktop/API research as ADRs.

Exit: non-Qt workspace crates build on Linux, Windows, and macOS; the QML shell builds on Linux.

## Stage 1 — Local wallpaper vertical slice

- Enumerate displays through the Qt/platform boundary.
- Persist stable display identities and physical arrangements.
- Decode local images with orientation and resource limits.
- Implement cover, contain, focal point, zoom, and simple per-display output.
- Implement Plasma 6 apply and Windows apply backends.
- Connect the QML monitor preview and local thumbnail grid to real models.

Exit: a local image can be previewed and applied without blocking the UI on Plasma 6 and Windows.

**Status:** Implemented for still images (Compose preview + Apply via Plasma 6 / Windows
backends, arrangement TOML persistence, CI GUI smoke screenshots). Live media remains Stage 6.

## Stage 2 — Physical composition

- PPI normalization and user physical-size override.
- Bezel correction, display groups, irregular arrangements, and rotation.
- Interactive drag, snapping, numeric editing, and before/after preview.
- Deterministic render plans and raster regression fixtures.
- Cache keys based on source content, profile, arrangement, and renderer version.

Exit: repeatable output for mixed-resolution, mixed-scale, and physically mismatched displays.

**Status:** Implemented for still images (physical-span planner with PPI/bezel
correction, layout fixtures, arrangement editing with snap + size/bezel overrides,
Compose Correction mode, cache keys including arrangement geometry). Perspective
correction remains Stage 7.

## Stage 3 — Library and compliant discovery

- Local folder index with filesystem watching.
- Openverse search adapter and source/license filters.
- Wikimedia Commons and NASA adapters if they add metadata or content not adequately exposed by
  Openverse.
- Collections, favorites, history, provenance, attribution, and cache management.
- Resolution/aspect suitability scoring for a selected display group.

Exit: users can discover and set high-quality online images while retaining required provenance.

**Status:** Implemented for still images (SQLite library index + folder watch, Openverse search
with license filters and provenance retention, acquisition cache with host allowlist, favorites/
history/collections persistence, suitability scoring against the active display group, Discover
and Library wired into Compose). Direct Wikimedia Commons and NASA adapters remain deferred while
Openverse covers those sources; dedicated adapters can land when they add metadata Openverse does
not expose.

## Stage 4 — Profiles and automation

- Profile editor and reusable display-group assignments.
- Interval, time-of-day, sunrise/sunset, and calendar-like schedules.
- Independent rotation queues, history, avoid-repeat behavior, pause, and skip.
- Tray controls and CLI commands.
- Display hotplug policy and automatic recovery from missing outputs.

Exit: unattended, explainable wallpaper rotation survives restart and topology changes.

**Status:** Implemented for still images (TOML automation catalog with profiles, display
groups, rotation queues, and schedules; `easel-scheduler` interval / time-of-day /
sunrise-sunset / calendar evaluation with avoid-repeat selection; pause / skip / tick CLI;
toolbar tray-equivalent pause/skip controls; hotplug missing-output policy applied on Compose
automation and display rematch). Native `SystemTrayIcon` awaits a Qt Widgets/`QApplication`
host; dynamic-still authoring remains Stage 5.

## Stage 5 — Dynamic stills

- Time-of-day and solar-keyed still sets with deterministic fallback frames.
- Optional cross-fade only where the active desktop backend can present it without a live host.
- Pre-render upcoming transitions and atomically replace completed output.
- Time-zone, daylight-saving, suspend/resume, and missed-transition behavior.
- Dynamic-still authoring and a timeline preview in the Qt interface.

Exit: a scheduled still set remains correct across restart, sleep, clock changes, and display
topology changes on every supported static backend.

## Stage 6 — Live media

- Local animated-image and video metadata, bounded poster extraction, and library thumbnails.
- Qt Multimedia preview with explicit runtime codec diagnostics.
- Shared playback clock and multi-display crop/transform compositor.
- KDE Plasma QML wallpaper plugin and lifecycle integration.
- Battery, full-screen application, lock, sleep/wake, and thermal pause policies.
- Windows and macOS live-host feasibility spikes; enable only backends that meet stability gates.
- Static poster fallback and crash recovery for every live session.

Exit: silent local motion media spans multiple displays without visible drift on at least one
supported live backend, within documented CPU/GPU and power budgets.

## Stage 7 — Platform breadth and correction

- Additional Linux desktops based on explicit backend capability tests.
- macOS backend and packaging.
- Perspective/viewer correction with a dedicated calibration experience.
- Workspace/activity support only where stable public interfaces exist.
- Lock-screen support only where authorized platform APIs permit it.

Exit: published support matrix is backed by automated tests and manual validation evidence.

## Stage 8 — Production hardening

- Accessibility audit, translations, performance budgets, and cancellation stress tests.
- Corrupt/hostile image and media corpora, codec-failure testing, and network-failure testing.
- Signed installers/packages, update policy, SBOM, and reproducible-build investigation.
- Privacy review and provider compliance review.
- Public documentation and support diagnostics.

Exit: 1.0 release criteria in `PRODUCT.md` are met.
