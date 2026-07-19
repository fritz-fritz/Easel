# easel-deps status (Easel tree)

Canonical consumer-facing notes for agents working in the Easel repo. The live
repo is https://github.com/fritz-fritz/easel-deps — keep this scaffold in
`tools/easel-deps/` in sync and push there when changing the release pipeline.

## Correctness bug in `libheif-v1.23.1` (first cut)

The first published asset was named `…-1.23.1.zip` but was built from vcpkg tag
`2026.05.25`, whose `libheif` port is **1.21.2** (`heif_version.h` /
`installed/vcpkg/status` confirm this). Release notes claimed 1.23.1.

Fix (already reflected in this scaffold):

1. Pin `vcpkg.ref` to commit `33e5269bbfc24fb252bc48a3e624c8193afdccce` (first
   port bump to 1.23.1), or a later monthly tag once one exists.
2. Package script verifies status/`heif_version.h` against `libheif.version` and
   fails the release job on mismatch.
3. Omit `debug/` from the zip (`include_debug: false`).
4. Publish `*.zip.sha256` next to the asset.

## Apply this scaffold to the sibling repo

```bash
# From a checkout that has both repos (Cloud Agent layout):
cd /path/to/easel-deps
git checkout -b cursor/fix-libheif-release-<id>
rsync -a --delete --exclude .git --exclude pending-remote.patch \
  ../easel/tools/easel-deps/ ./
git add -A && git commit -m "fix(release): pin real libheif 1.23.1 and automate upstream sync"
git push -u origin HEAD
# open PR, merge, then:
gh workflow run build-libheif-windows.yml --repo fritz-fritz/easel-deps
```

Alternatively apply `tools/easel-deps/pending-remote.patch` with `git am`, or restore
from the bundle: `git clone -b main tools/easel-deps.bundle easel-deps-new`.

Then refresh Easel’s `.github/libheif-windows.lock.json` (or let
`.github/workflows/sync-easel-deps.yml` open the pin PR once the corrected
release publishes a `.sha256` sidecar).

## Sync loop

```
strukturag/libheif release
        │
        ▼
easel-deps  Sync libheif upstream  ──PR──► versions.json bump
        │
        ▼
easel-deps  Build libheif (Windows MSVC) ──► GitHub Release + SHA256
        │
        ▼
Easel       sync-easel-deps.yml ──PR──► .github/libheif-windows.lock.json
        │
        ▼
Easel CI    install-libheif-windows.ps1 (checksum + version header)
```

## Immutable releases and rebuild tags

GitHub immutable releases permanently burn a tag name even after deletion.
Corrected rebuilds therefore publish as `libheif-vX.Y.Z-rN` (e.g. `libheif-v1.23.1-r2`).

Do **not** loop `gh release create` while capturing its stdout into a PowerShell
variable — `gh` prints the release URL, so `$code = fn` becomes `@(url, 0)` and
`if ($code -eq 0)` is falsy, which previously minted r3…r20 in one job.
