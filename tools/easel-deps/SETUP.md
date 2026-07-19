# easel-deps status

Repo: https://github.com/fritz-fritz/easel-deps  
Latest release: [`libheif-v1.23.1`](https://github.com/fritz-fritz/easel-deps/releases/tag/libheif-v1.23.1)

Asset consumed by Easel CI:

`libheif-msvc-x64-windows-static-md-1.23.1.zip`

(Note: at vcpkg tag `2026.05.25` the `libheif` port resolves to **1.21.2**; the release tag/asset name tracks `versions.json`.)

## Rebuild / bump

1. Edit `versions.json` in the easel-deps repo (or sync from this tree).
2. Run Actions → **Build libheif (Windows MSVC)**, or push tag `libheif-vX.Y.Z`.
3. Keep `.github/scripts/install-libheif-windows.ps1` pins in sync.
