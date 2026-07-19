# ADR 0009: Own a libheif prebuild repo for Windows (and optional macOS) CI

- Status: accepted
- Date: 2026-07-19

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

Canonical scaffold (until agents have push access to that repo) lives in
`tools/easel-deps/` with a git bundle at `tools/easel-deps.bundle` — see
`tools/easel-deps/SETUP.md`.

1. On tag / schedule / workflow_dispatch, builds **MSVC** `x64-windows-static-md`
   libheif with the codecs Easel needs (`libde265`, `x265`, `aom`).
2. Publishes a versioned GitHub Release zip with a **stable layout**, preferably
   already shaped for `libheif-sys` + `vcpkg-rs`:
   ```
   .vcpkg-root
   installed/x64-windows-static-md/{include,lib}/…
   installed/vcpkg/{status,info/*.list,updates/}
   ```
3. Pins exact upstream versions in the release notes / a `versions.json`.
4. Easel CI downloads that asset (checksum-verified) instead of compiling or
   depending on an unaffiliated third-party binary host.

Linux/macOS keep apt/brew. Optional later: same repo can publish macOS bottles if
Homebrew becomes a bottleneck.

## Consequences

- Easel CI Windows jobs stay in the “download + link” regime (~seconds).
- Codec/CRT/library naming is under our control (avoids `x265-static` vs `x265`,
  missing `aom`, `/MD` mismatches).
- One more repo to maintain; schedule rebuilds when bumping libheif.
- Until the sibling repo exists, CI may use a documented interim zip with a
  cache-bust key so old vcpkg link lines do not linger in `rust-cache`.

## References

- https://github.com/strukturag/libheif/releases (source-only assets)
- Pillow `winbuild/` + wheels workflow
- `libheif-sys` Windows path (`vcpkg::find_package("libheif")`)
- `.github/scripts/install-libheif-windows.ps1`
