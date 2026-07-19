# AGENTS.md

## Cursor Cloud specific instructions

Easel is a Rust (edition 2024, pinned to toolchain `1.85` via `rust-toolchain.toml`)
workspace. It has two distinct build scopes:

- Default-member crates (`crates/easel-*`) are pure Rust and need no system libraries.
  Standard commands work as documented in `README.md` / `CONTRIBUTING.md`:
  `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all --check`.
- The `easel-desktop` app (`apps/easel-desktop`) is a Qt 6 + CXX-Qt GUI and needs the
  Qt6/C++ toolchain (baked into the Cloud VM: `qt6-base-dev`, `qt6-declarative-dev`,
  the `qml6-module-qtquick*` modules, `ninja-build`, `clang`, `xvfb`). Build/lint it
  with `CXX=g++ CC=gcc` (matching CI), e.g. `CXX=g++ CC=gcc cargo build -p easel-desktop`.

### Related repositories (multi-repo)

Easel coordinates with two sibling repos under the same owner (referenced in
`docs/ci-visual-assets-repo.md` and `docs/adr/0009-libheif-prebuilt-deps.md`):

- `github.com/fritz-fritz/easel-ci-visual` — CI visual galleries / GitHub Pages.
- `github.com/fritz-fritz/easel-deps` — prebuilt Windows libheif for CI.

They are listed in `.cursor/environment.json` under `repositoryDependencies`, which is
what scopes the Cloud Agent's generated GitHub token to include them so the agent can
read and push. Token scoping alone is not enough: the Cursor GitHub App must also be
authorized on each sibling repo (a one-time owner action in GitHub settings). Sibling
repos are not auto-cloned into `/workspace`; clone on demand (e.g.
`git clone https://github.com/fritz-fritz/easel-deps`) when you need to push to them.

### Non-obvious gotchas

- `cxx-qt-build` unconditionally forces `-fuse-ld=gold` on Linux. On this VM image the
  gold linker only finds `libstdc++.so` under `/usr/lib/gcc/x86_64-linux-gnu/13/`
  (the plain `/usr/lib/x86_64-linux-gnu` ships only `libstdc++.so.6`), so a plain
  desktop build fails with `ld.gold: cannot find -lstdc++`. A VM-global cargo config at
  `/usr/local/cargo/config.toml` adds that dir to the linker search path for the
  `x86_64-unknown-linux-gnu` target, so `cargo build -p easel-desktop` links out of the
  box. This lives in the VM (not the repo); do not delete it. If you ever build from a
  clean env, the equivalent is `RUSTFLAGS="-L native=/usr/lib/gcc/x86_64-linux-gnu/13"`.

### Running the GUI

- Headless smoke render (what CI validates). Always writes the fixture multi-monitor
  preview (`gui-preview.png`) and, by default, a full-window Compose screenshot
  (`gui-compose.png`). Pass `--smoke-views` to choose pages (or `all`):

  ```
  QT_QPA_PLATFORM=xcb QT_QUICK_CONTROLS_STYLE=Fusion CXX=g++ CC=gcc xvfb-run -a \
    cargo run -p easel-desktop -- --smoke-screenshot <outdir> \
    --smoke-views preview,compose
  ```

  CI picks views from the PR/push diff via `.github/ci-visual/select_smoke_views.py`.
- Interactive: an XFCE desktop is available on `DISPLAY=:1`. Launch the full app with
  `DISPLAY=:1 QT_QUICK_CONTROLS_STYLE=Fusion CXX=g++ CC=gcc cargo run -p easel-desktop`.
  It enumerates the live X screen (the VNC display) rather than the smoke fixture layout.

### Multi-display (3 monitors) on the live desktop

- The live app enumerates `Qt.application.screens` (XRandR-backed), so the number of
  displays it reports equals the number of RandR monitors on `DISPLAY=:1`. The VNC
  framebuffer is a single output (`VNC-0`), i.e. **one** display by default.
- To exercise multi-display handling like CI's `DP-1`/`DP-2`/`DP-3` fixture, run
  `tools/dev/three-displays.sh` — it uses `xrandr --setmonitor` to split the framebuffer
  into three staggered logical monitors (`DP-1`/`DP-2`/`DP-3`, CI-matching physical mm).
  `tools/dev/three-displays.sh reset` restores the single monitor. The script is
  idempotent and defensive, and is wired into `.cursor/environment.json` `start`, so fresh
  Cloud VMs come up with three monitors already defined. If you launch the app *before*
  splitting (or change the split while it is open), click **Refresh displays** to re-probe.
  The split is a live X-server change and is not persisted across VNC restarts; re-run the
  script (or rely on `start`) after a restart.
- Actually *setting* the wallpaper on Linux is implemented **only** for KDE Plasma 6
  (`crates/easel-platform/src/plasma.rs`, via `qdbus`/`org.kde.plasmashell`); there is no
  XFCE/GNOME/generic-X backend. On the XFCE Cloud desktop `select_wallpaper_backend()`
  returns `NoBackend`, so the Compose **Apply** button cannot push to the compositor here.
  CI does not set real wallpaper either — it validates the per-display **apply-payload
  rasters** that Easel would hand the compositor. Reproduce those (three `apply-display-*.png`
  for the `DP-1/2/3` fixture) with:

  ```
  EASEL_VISUAL_OUTDIR=<outdir> cargo test -p easel-render write_apply_payload_visual_artifacts
  ```
- Headless automation CLI (shared store with the desktop app):

  ```
  cargo run -p easel-cli -- status
  cargo run -p easel-cli -- pause
  cargo run -p easel-cli -- resume
  cargo run -p easel-cli -- skip
  cargo run -p easel-cli -- next
  cargo run -p easel-cli -- profiles
  cargo run -p easel-cli -- schedules
  cargo run -p easel-cli -- stills
  cargo run -p easel-cli -- inspect-heic path/to/Dynamic.heic
  cargo run -p easel-cli -- import-heic path/to/Dynamic.heic --name Mojave
  ```

- `easel-dynamic` (HEIC import/encode) needs `libheif-dev` at build time plus an encoder plugin
  (`libheif-plugin-x265` and/or `libheif-plugin-aomenc`). The Cloud VM image includes these;
  on a clean host install `libheif-dev` and the encoder/decoder plugins. On Windows CI/MSVC,
  run `.github/scripts/install-libheif-windows.ps1` (prebuilt static libs staged for
  `libheif-sys`; upstream GitHub releases are source-only).
