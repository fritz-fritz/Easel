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
`cargo fmt` runs once on Ubuntu; `cargo test` and `cargo clippy` still run on Linux, Windows, and
macOS. A dedicated multi-OS desktop job builds `easel-desktop`, runs `--smoke-screenshot`, and
publishes Qt GUI PNGs as artifacts on Linux (Xvfb), Windows, and macOS. Core jobs also write
per-display apply-payload raster PNGs for visual review. Full packaging and code signing remain
separate later stages. Live wallpaper Apply against a real desktop session is still a manual
matrix item; CI never mutates an operator wallpaper.

Media CI adds only short, project-owned, silent fixtures. Codec assertions are capability-aware so
the suite distinguishes an unavailable runtime decoder from an incorrect compositor result.

### Visual harness stages

Producers write stage-local PNGs under a temp directory. The composite action
[`.github/actions/ci-visual`](../.github/actions/ci-visual) renames, uploads (`archive: false`),
emits a JSON manifest, and summarizes them. New stages only need a producer plus one
`uses: ./.github/actions/ci-visual` block with a distinct `stage` / `pattern`.

| Stage id | Producer | Gate | Expected files | Published via |
| --- | --- | --- | --- | --- |
| `apply-payload` | [`crates/easel-render/tests/visual_artifacts.rs`](../crates/easel-render/tests/visual_artifacts.rs) | `EASEL_VISUAL_OUTDIR` set (CI only) | `apply-display-*.png` | `ci-visual` |
| `gui-smoke` | `easel-desktop --smoke-screenshot <dir>` | smoke flag / out dir | `gui-*.png` | `ci-visual` |

### PR gallery (dual surface)

After the `CI` workflow finishes on a pull request, [`ci-visual-gallery.yml`](../.github/workflows/ci-visual-gallery.yml):

1. Downloads visual artifacts + manifests
2. Builds a styled HTML gallery via [`.github/ci-visual/build_gallery.py`](../.github/ci-visual/build_gallery.py)
3. Publishes it to the separate Pages repo `fritz-fritz/easel-ci-visual` (when
   `EASEL_CI_VISUAL_TOKEN` is configured)
4. Upserts a sticky PR comment that includes **both** an inline Markdown table gallery and a
   link to the hosted HTML gallery

CI visual PNGs/HTML must not be committed to branches of this source repository. Setup details:
[ci-visual-assets-repo.md](ci-visual-assets-repo.md).

Cursor Cloud Agent **Demo** artifacts are complementary and agent-scoped; Actions cannot write
into Demo. Prefer CI galleries for every PR.

Public workflow artifacts remain non-installable synthetic review images. See
[distribution policy](DISTRIBUTION.md).
