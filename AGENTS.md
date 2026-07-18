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
  on a clean host install `libheif-dev` and the encoder/decoder plugins.
