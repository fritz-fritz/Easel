# ADR 0009: Own a libheif prebuild repo for Windows (and optional macOS) CI

- Status: accepted
- Date: 2026-07-19
- Updated: 2026-07-19 (release accuracy + upstream sync)

## Context

`easel-dynamic` links `libheif` via `libheif-sys`. On Linux/macOS, distro/Homebrew
packages are fast. On Windows MSVC, options are:

| Approach | CI cost | Control |
| --- | --- | --- |
| `cargo-vcpkg` from source | 10–20+ min; flaky | Full |
| Third-party zip (e.g. vegidio/binaries-heif) | Seconds | Weak (names, codecs, CRT) |
| **Owned prebuild + GitHub Release** | Seconds download | Full |

Pillow builds native deps in CI/`winbuild` (and related wheel pipelines) so
application CI never compiles libjpeg/libtiff/etc. from scratch. `pillow_heif`
similarly caches or vendors libheif builds. Easel should do the same for the
one expensive Windows dependency that is not in the Qt installer.

Upstream `strukturag/libheif` releases are **source tarballs only** — there is no
official Windows binary asset to pin.

## Decision

Use sibling repo [`fritz-fritz/easel-deps`](https://github.com/fritz-fritz/easel-deps) that:

Canonical scaffold lives in `tools/easel-deps/` with a git bundle at
`tools/easel-deps.bundle` — see `tools/easel-deps/SETUP.md`.

1. Builds **MSVC** `x64-windows-static-md` libheif (`libde265`, `x265`, `aom`) from a
   pinned microsoft/vcpkg **tag or commit** in `versions.json`.
2. **Verifies** the installed port / `heif_version.h` matches `libheif.version`
   before publishing (prevents tag/asset name drift).
3. Publishes a versioned GitHub Release zip (+ `.sha256` sidecar) shaped for
   `libheif-sys` / `vcpkg-rs` (release layout omits `debug/` by default):
   ```
   .vcpkg-root
   versions.json
   installed/x64-windows-static-md/{include,lib,share}/…
   installed/vcpkg/{status,info/*.list,updates/}
   ```
4. **Sync libheif upstream** (scheduled) opens PRs when strukturag/libheif and
   microsoft/vcpkg both expose a newer port.
5. Easel pins the asset in `.github/libheif-windows.lock.json`. CI installs via
   `.github/scripts/install-libheif-windows.ps1` (SHA-256 + version header
   verified). **sync-easel-deps** opens PRs when easel-deps publishes a corrected
   release (requires `.sha256` sidecar).

Linux/macOS keep apt/brew. Optional later: same repo can publish macOS bottles if
Homebrew becomes a bottleneck.

## Consequences

- Easel CI Windows jobs stay in the “download + link” regime (~seconds).
- Codec/CRT/library naming is under our control (avoids `x265-static` vs `x265`,
  missing `aom`, `/MD` mismatches).
- One more repo to maintain; upstream sync + weekly rebuild keep pins honest.
- The first `libheif-v1.23.1` cut was mispackaged (vcpkg tag `2026.05.25` → port
  **1.21.2** under a 1.23.1 name). Fixed publisher pins vcpkg commit
  `33e5269b…` (port 1.23.1), verifies before release, and Easel rejects assets
  without a `.sha256` sidecar / mismatched `heif_version.h` (interim third-party
  fallback remains until the corrected release exists).

## References

- https://github.com/strukturag/libheif/releases (source-only assets)
- Pillow `winbuild/` + wheels workflow
- `libheif-sys` Windows path (`vcpkg::find_package("libheif")`)
- `.github/scripts/install-libheif-windows.ps1`
- `.github/libheif-windows.lock.json`
- `.github/workflows/sync-easel-deps.yml`
