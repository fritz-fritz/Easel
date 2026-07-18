# ADR 0005: Dynamic still sets evaluate frames independently of rotation queues

- Status: accepted
- Date: 2026-07-18

## Context

Stage 5 requires time-of-day and solar-keyed still sequences that stay correct across restart,
sleep, clock changes, and display topology changes. Stage 4 already provides schedules, rotation
queues, apply history, and a static `WallpaperBackend` path. Dynamic stills are not continuous
media (Stage 6) and must not invent a second apply pipeline.

## Decision

1. **Domain types live in `easel-core`.** A `DynamicStillSet` owns ordered keyed frames
   (`TimeOfDay` or `Solar` plus offset), a required fallback asset, observer lat/lon for solar
   keys, and an optional `request_cross_fade` flag. Pure evaluators
   (`active_frame_at`, `next_transition_after`, `decide_transition`) take an injected clock and
   fixed UTC offset, matching Stage 4 schedule evaluation.

2. **Profiles reference an optional `still_set_id` when `presentation` is `DynamicStills`.**
   Schema version 3 migrates older profiles without still sets. Rotation queues and schedules
   remain available for the same profile; frame selection does not advance the queue cursor.

3. **`easel-scheduler` persists still sets as TOML and records last-applied frame state in
   SQLite.** On each poll the desktop runtime evaluates due dynamic profiles, applies only when
   the active key/asset changed (missed-transition catch-up), and pre-renders the next transition
   into an atomically promoted staging path before apply when possible.

4. **Cross-fade is capability-gated.** Backends report `BackendCapabilities::cross_fade`. Plasma
   and Windows still backends leave it false, so requested fades degrade to a hard cut while
   remaining on the static wallpaper path.

5. **Compose authors a dense hourly placeholder** when Media=Dynamic stills is saved without a
   HEIC import, and exposes a timeline scrub preview. Prefer importing an Apple/Plasma dynamic
   HEIC (`easel inspect-heic` / future Compose import) for solar-position or appearance sets.
   Live media remains rejected until Stage 6.

## Consequences

- Dynamic still correctness is unit-testable without a desktop session.
- Suspend/resume and forward clock jumps converge on the current frame once; intermediate frames
  are skipped intentionally.
- CLI `status` / `stills` share the same store as the Automation poller.
- ADR 0006 supersedes the sparse morning/noon/evening framing: Apple HEIC is the interchange
  format, and native per-display bundles are the preferred apply path where backends allow.

## References

- `docs/ROADMAP.md` Stage 5
- `docs/adr/0003-dynamic-and-live-wallpapers.md`
- `docs/adr/0004-schedules-rotation-hotplug.md`
