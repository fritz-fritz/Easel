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

Layout inside the zip:

```
.vcpkg-root
installed/x64-windows-static-md/{include,lib,share}/…
installed/vcpkg/{status,info/*.list,updates/}
```

Set `VCPKG_ROOT` to the extracted directory before `cargo build`.

## Versions

Pinned in [`versions.json`](versions.json). Bump the libheif / vcpkg pins there,
then run the **Build libheif (Windows MSVC)** workflow (or push a tag
`libheif-vX.Y.Z`).

## Local use (Easel)

```powershell
# after a release exists:
gh release download libheif-v1.23.1 --repo fritz-fritz/easel-deps `
  --pattern "libheif-msvc-*.zip" --dir $env:RUNNER_TEMP
Expand-Archive … 
$env:VCPKG_ROOT = "<extract-root>"
```

Easel wires this via `.github/scripts/install-libheif-windows.ps1`.

## License

Build scripts: MPL-2.0. Release binaries: upstream licenses of packaged libraries.
