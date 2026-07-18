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
macOS. A dedicated multi-OS desktop job builds `easel-desktop`, runs `--smoke-screenshot`
with `--smoke-views` selected from the PR/push diff, and publishes Qt GUI PNGs as
artifacts on Linux (Xvfb), Windows, and macOS. Core jobs also write
per-display apply-payload raster PNGs for visual review. Full packaging and code signing remain
separate later stages. Live wallpaper Apply against a real desktop session is still a manual
matrix item; CI never mutates an operator wallpaper.

### Path filters

[`ci.yml`](../.github/workflows/ci.yml) uses `dorny/paths-filter` so expensive jobs only run when
the diff can affect them:

| Output | Runs | Typical paths |
| --- | --- | --- |
| `rust` | Core matrix (fmt/test/clippy + apply-payload visuals) | `crates/**`, `apps/**`, `Cargo.*`, rust toolchain/fmt config |
| `desktop` | Qt desktop smoke matrix + gui-smoke visuals | `apps/easel-desktop/**`, `crates/**`, `Cargo.*`, `.github/actions/ci-visual/**` |
| `gallery_tooling` | Gallery builder unit tests | `.github/ci-visual/**`, gallery/action workflows, `ci.yml` |

Docs-only or unrelated `.github` docs changes skip core and desktop. With no visual artifacts, the
`workflow_run` gallery publisher exits early (and still posts a successful
`CI Visual Gallery / OS compare` status). Prefer requiring the aggregate **`CI gate`** check in
branch protection rather than individual matrix legs, so skipped jobs do not block merges.

Media CI adds only short, project-owned, silent fixtures. Codec assertions are capability-aware so
the suite distinguishes an unavailable runtime decoder from an incorrect compositor result.

### Visual harness stages

Producers write stage-local PNGs under a temp directory. The composite action
[`.github/actions/ci-visual`](../.github/actions/ci-visual) renames them, writes a JSON
manifest, and uploads **one zip artifact per stage×OS** named `ci-visual-<stage>-<os>`.
The gallery publisher downloads those bundles (`pattern: ci-visual-*`), unpacks them, and
builds the dual review surfaces. New stages only need a producer plus one
`uses: ./.github/actions/ci-visual` block with a distinct `stage` / `pattern`.

When SHA-256 digests differ for `apply-payload`, the gallery builder decodes the PNGs and
classifies ±1 LSB channel drift as `match-tolerant`. That is **diagnostic only** — the OS
compare gate still fails because the goal is byte-identical output across runners. Larger
drift is `content-mismatch`.

Still-image resampling uses a portable Lanczos-3 implementation (`easel-render::resize`) backed
by the pure-Rust `libm` crate instead of the `image` crate's platform `sin`, so MSVC and Unix
runners produce matching apply-payload bytes.

| Stage id | Producer | Gate | Expected files | Published via |
| --- | --- | --- | --- | --- |
| `apply-payload` | [`crates/easel-render/tests/visual_artifacts.rs`](../crates/easel-render/tests/visual_artifacts.rs) | `EASEL_VISUAL_OUTDIR` set (CI only) | `apply-display-*.png` | `ci-visual` |
| `gui-smoke` | `easel-desktop --smoke-screenshot <dir> --smoke-views …` | smoke flag / out dir | `gui-*.png` (fixture `gui-preview.png` plus selected full-window `gui-<view>.png`) | `ci-visual` |

### Selective GUI smoke views

Desktop smoke always captures the fixture multi-monitor **preview** (`gui-preview.png`, the
`MonitorPreview` grab). It also captures full-window screenshots for workspace pages that the
diff may have affected (`compose`, `discover`, `library`, `profiles`, `automation`).

[`.github/ci-visual/select_smoke_views.py`](../.github/ci-visual/select_smoke_views.py) maps
changed paths → a comma-separated `--smoke-views` list. Shared shell / `crates/**` changes
select every page; a Discover-only controller change selects `preview,discover`. Local default
when the flag is omitted is `preview,compose`.

### PR gallery (dual surface)

After the `CI` workflow finishes on a pull request, [`ci-visual-gallery.yml`](../.github/workflows/ci-visual-gallery.yml):

1. Lists `ci-visual-*` zip bundles on the triggering CI run; if none, exits success (skips publish)
2. Downloads and unpacks those bundles (`download-artifact` + `merge-multiple`)
3. Builds a styled HTML gallery via [`.github/ci-visual/build_gallery.py`](../.github/ci-visual/build_gallery.py),
   including per-asset metadata (dimensions, bytes, SHA-256) and a **cross-OS comparison**
4. Publishes it to the separate Pages repo `fritz-fritz/easel-ci-visual` (when
   `EASEL_CI_VISUAL_TOKEN` is configured)
5. Upserts a sticky PR comment with inline thumbnails, metadata, comparison summary, and a
   link to the hosted HTML gallery (`raw.githubusercontent.com` embeds so camo does not race Pages)
6. Posts a commit status **`CI Visual Gallery / OS compare`** on the PR head SHA (and fails the
   gallery job when `apply-payload` assets content/size-mismatch across OS)

**Cross-OS compare rules**

| Stage | Expectation | Gate |
| --- | --- | --- |
| `apply-payload` | Byte-identical PNGs across `ubuntu` / `windows` / `macos` for each display | Fail on digest/pixel mismatch (including ±1 LSB near-matches, which are labeled `match-tolerant` for debugging) or size mismatch |
| `gui-smoke` | Platform chrome differs | Informational only (hashes/dims still shown) |

Incomplete OS matrices (an asset present on some runners only) are warnings, not hard failures.

CI visual PNGs/HTML must not be committed to branches of this source repository. Setup details:
[ci-visual-assets-repo.md](ci-visual-assets-repo.md).

#### Required checks vs `workflow_run` / `workflow_dispatch`

The gallery publisher is **`workflow_run`-driven** (not a `pull_request` job and not
`workflow_dispatch`). That means GitHub will not list the gallery workflow itself as a classic
PR check produced by the PR head workflow file.

To still require the visual OS compare before merge:

1. Require the aggregate **`CI gate`** check from [`ci.yml`](../.github/workflows/ci.yml)
   (path filters may skip core/desktop; the gate stays green when those jobs are intentionally
   skipped)
2. In branch protection / rulesets, also require the commit status context
   **`CI Visual Gallery / OS compare`** (posted by the gallery workflow onto the PR head SHA;
   succeeds immediately when the CI run produced no visual artifacts)

A **`workflow_dispatch`-only** workflow cannot be a meaningful required PR check: it does not run
automatically on each PR push, so branch protection cannot rely on it. Optional manual
`workflow_dispatch` is fine for republish/backfill helpers (see the archive workflow), but the
merge gate should stay on automatic `pull_request` jobs and/or commit statuses from
`workflow_run`.

Cursor Cloud Agent **Demo** artifacts are complementary and agent-scoped; Actions cannot write
into Demo. Prefer CI galleries for every PR.

Public workflow artifacts remain non-installable synthetic review images. See
[distribution policy](DISTRIBUTION.md).
