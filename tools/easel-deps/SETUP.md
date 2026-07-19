# Push this tree to github.com/fritz-fritz/easel-deps

The Cursor cloud agent can write to the Easel repo but not to `easel-deps`
until that repository is added to the same GitHub App / token scope.

## Option A — git bundle (recommended)

From an Easel checkout that contains `tools/easel-deps.bundle`:

```bash
git clone https://github.com/fritz-fritz/easel-deps.git
cd easel-deps
git pull /path/to/Easel/tools/easel-deps.bundle main
git push -u origin main
```

## Option B — copy this directory

```bash
git clone https://github.com/fritz-fritz/easel-deps.git
cd easel-deps
cp -a /path/to/Easel/tools/easel-deps/. .
rm -f SETUP.md   # optional; Easel-only helper
git add -A
git commit -m "Initial easel-deps: Windows MSVC libheif prebuild pipeline"
git push -u origin main
```

## After the first push

1. GitHub → easel-deps → Actions → **Build libheif (Windows MSVC)** → Run workflow
   (or: `git tag libheif-v1.23.1 && git push origin libheif-v1.23.1`).
2. Confirm release asset `libheif-msvc-x64-windows-static-md-1.23.1.zip`.
3. Easel CI downloads it via `.github/scripts/install-libheif-windows.ps1`.

Granting the Cursor GitHub App access to `easel-deps` lets future agents push and
trigger builds directly.
