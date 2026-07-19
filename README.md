# Easel

Easel is a greenfield, cross-platform wallpaper manager for complex multi-monitor
desktops. It combines physically correct spanning with a modern Qt 6 interface and a
policy-aware catalog of high-quality, reusable images. Its architecture also distinguishes
scheduled dynamic stills from persistent animated-image and video wallpapers.

This repository contains the Stage 1–5 still-image vertical slice: decode/fit/raster with
physical multi-display composition, Compose previews, Qt display enumeration with arrangement
persistence, Plasma 6 / Windows still apply backends, local library indexing, Openverse
discovery with retained provenance, reusable profiles with schedule-driven rotation,
time-of-day / solar-position dynamic still sets (Apple HEIC interchange) with catch-up and
pre-render, and hotplug policy. Use
Compose → Open image → Apply (or Save profile for automation), or Discover/Library to select an
image first. CI captures apply-payload rasters plus selective Qt GUI smoke screenshots (fixture
preview and affected workspace pages) for review. Animated/live media hosts and additional Linux
desktops remain deliberately unimplemented.

## Product direction

- Span one image across arbitrary monitor arrangements.
- Correct for mixed pixel density, physical display size, bezels, rotation, and perspective.
- Assign independent images or shared images to arbitrary display groups.
- Keep logical desktop coordinates separate from native pixels under fractional scaling.
- Browse local collections and compliant online catalogs from one visual library.
- Preserve creator, source, and license attribution throughout discovery and use.
- Run slideshows based on intervals, wall-clock schedules, events, and rules.
- Present local animated images and silent video through capability-checked live backends.
- Fall back to a generated poster frame when live playback is unavailable or fails.
- Provide a responsive Qt Quick interface on Linux, Windows, and macOS.

Easel is inspired by the capabilities of Superpaper, but it is not a port and does not
reuse the prior codebase or configuration model.

## Repository layout

```text
apps/easel-desktop/     Qt Quick application and CXX-Qt boundary
apps/easel-cli/         Headless profile/schedule/status/pause/skip controls
crates/easel-core/      Versioned domain model and validation
crates/easel-library/   Local folder index, SQLite library store, acquisition cache
crates/easel-scheduler/ Automation TOML store and SQLite rotation history
crates/easel-dynamic/   Apple HEIC dynamic import/encode and per-display native bundles
crates/easel-render/    Display-space planning, raster output, and live frame plans
crates/easel-providers/ Online image provider contracts and adapters
crates/easel-platform/  Static wallpaper and persistent live-host contracts
docs/                   Product, architecture, provider, and delivery plans
```

Dynamic still interchange follows Apple Dynamic Desktop HEIC (solar / appearance / h24). Easel
deconstructs packages into a portable still set, encodes per-display native HEIC packages for
platforms that can host them, and falls back to still-frame apply elsewhere (see ADR 0006).

## Development

The non-Qt workspace crates are the default Cargo members:

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

The desktop application additionally requires Qt 6 Core, Gui, Qml, Quick, and Quick
Controls plus a C++ toolchain. On openSUSE Tumbleweed, the package names are generally:

```sh
sudo zypper install rust cargo clang cmake ninja \
  qt6-base-devel qt6-declarative-devel
cargo run -p easel-desktop
```

Qt Multimedia becomes a desktop dependency when the live playback stage is implemented; it is
not linked by the current UI-only scaffold. Distribution package names can change; the CI
workflow is the canonical Ubuntu setup.

Read [the product plan](docs/PRODUCT.md), [architecture](docs/ARCHITECTURE.md),
[dynamic and live wallpaper plan](docs/LIVE_WALLPAPERS.md),
[image provider policy](docs/IMAGE_PROVIDERS.md), and [roadmap](docs/ROADMAP.md) before adding
implementation.

## Builds and distribution

The source code is public under the [Mozilla Public License 2.0](LICENSE). Anyone may build it
subject to that license. Official Easel packages will be signed and sold through designated
storefronts; that purchase funds convenient installation, trusted updates, and project support.

Public CI intentionally publishes only non-installable test screenshots. It does not publish
application packages, signing material, or store credentials. See
[distribution policy](docs/DISTRIBUTION.md) and [trademark guidance](TRADEMARKS.md).

Easel is pre-alpha. Do not copy code from the reference project into this repository; ideas and
observable behavior may be used as design input.
