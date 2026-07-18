# ADR 0004: Schedules, rotation queues, and hotplug policy

- Status: accepted
- Date: 2026-07-18

## Context

Stage 4 requires unattended wallpaper rotation that survives restart and display topology
changes. Profiles already describe composition, but they do not encode timing, queue
membership, avoid-repeat behavior, or what to do when a profile references a missing
output. Dynamic stills (Stage 5) and live media (Stage 6) both assume a scheduler exists.

## Decision

1. **Domain types live in `easel-core`.** Schedules, rotation queues, selection rules, and
   hotplug policy are pure, versioned, serializable types with deterministic evaluation that
   takes an injected clock and UTC offset. Solar events use a NOAA-style approximation; they
   are good enough for wallpaper transitions and covered by unit tests.

2. **`easel-scheduler` owns persistence and runtime helpers.** Profiles, display groups,
   schedules, rotation queues, and hotplug policy are human-editable TOML with atomic writes.
   Rotation apply history and last-fire timestamps use SQLite so avoid-repeat queries stay
   cheap. The crate has no Qt dependency so `easel-cli` and `easel-desktop` share it.

3. **Profiles reference optional group, queue, and schedule ids.** Compose “Save profile”
   creates a profile, a one-asset queue from the current local image, and a schedule derived
   from the Manual / Every hour / Time of day control. Display groups remain reusable named
   membership lists stored beside profiles.

4. **Hotplug policy is explicit.** On topology change the arrangement is rematched first.
   Then each profile is resolved with one of: skip missing outputs, defer until the full set
   returns, or apply using all currently connected displays. The saved arrangement is never
   rewritten by a missing-output decision.

5. **Tray-equivalent controls and CLI share the same store.** Pause, resume, skip, and status
   mutate or read the same TOML/SQLite documents. The Automation page and `easel` CLI expose
   those actions. A native `SystemTrayIcon` is deferred until the desktop host can construct a
   Qt Widgets `QApplication` (`cxx-qt-lib` currently provides `QGuiApplication` only).

## Consequences

- Interval, wall-clock, solar, and weekday schedules can be tested without a desktop session.
- Stage 5 dynamic-still timelines reuse the same schedule evaluator and apply history.
- Collection-backed queues resolve membership through `easel-library` at selection time.
- The desktop background poller selects, renders, and applies due stills through
  `WallpaperBackend`, committing rotation history only after a successful apply. The
  `easel` CLI remains a shared control plane (status/pause/resume/skip/next) without
  linking the Qt/render stack.

## References

- `docs/ROADMAP.md` Stage 4
- `docs/ARCHITECTURE.md` persistence and core ownership
- `docs/adr/0003-dynamic-and-live-wallpapers.md`
