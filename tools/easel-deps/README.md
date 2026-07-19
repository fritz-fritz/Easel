# easel-deps

Prebuilt native dependencies for [Easel](https://github.com/fritz-fritz/Easel) CI
and local Windows MSVC builds.

## Why

Upstream [`strukturag/libheif`](https://github.com/strukturag/libheif) ships
**source-only** GitHub releases. Compiling libheif (+ aom/x265/libde265) inside
Easel's PR CI is too slow. This repo builds once and publishes versioned zips
that Easel downloads in seconds (Pillow-style dependency prebuilds).

## Current artifacts

| Asset | Triplet | Contents |
| --- | --- | --- |
| `libheif-msvc-x64-windows-static-md-<ver>.zip` | `x64-windows-static-md` | libheif with HEVC + AVIF (aom), staged for `libheif-sys` / `vcpkg-rs` |
| `*.zip.sha256` | — | SHA-256 sidecar for the zip |

Layout inside the zip:

```
.vcpkg-root
versions.json          # pinned + resolved provenance
installed/x64-windows-static-md/{include,lib,share}/…
installed/vcpkg/{status,info/*.list,updates/}
```

Release packages omit `debug/` by default (`asset.include_debug: false`).

Set `VCPKG_ROOT` to the extracted directory before `cargo build`.

## Versions

Pinned in [`versions.json`](versions.json):

- `libheif.version` — expected port / release version (must match what vcpkg installs)
- `vcpkg.ref` + `vcpkg.ref_kind` — microsoft/vcpkg **tag** or **commit** that provides that port

The build workflow **refuses to publish** if the installed port version disagrees with
`libheif.version` (this is what made the first `libheif-v1.23.1` asset actually
contain 1.21.2 when `vcpkg.tag` was `2026.05.25`).

## Automation

| Workflow | Trigger | Purpose |
| --- | --- | --- |
| **Sync libheif upstream** | daily schedule + dispatch | When strukturag/libheif + microsoft/vcpkg both have a newer port, open a PR bumping `versions.json` |
| **Build libheif (Windows MSVC)** | `versions.json` merge / tag / weekly schedule / dispatch | Build, verify version, publish `libheif-vX.Y.Z` + SHA256 |

Manual bump:

1. Edit `versions.json` (or merge a sync PR).
2. Push to `main` (or tag `libheif-vX.Y.Z`).
3. Confirm the release asset + `.sha256` match the packaged port version.

## Local use (Easel)

```powershell
gh release download libheif-v1.23.1 --repo fritz-fritz/easel-deps `
  --pattern "libheif-msvc-*.zip*" --dir $env:RUNNER_TEMP
Expand-Archive …
$env:VCPKG_ROOT = "<extract-root>"
```

Easel pins the asset in `.github/libheif-windows.lock.json` and installs via
`.github/scripts/install-libheif-windows.ps1` (checksum + `heif_version.h` verified).

## License

Build scripts: MPL-2.0. Release binaries: upstream licenses of packaged libraries.
