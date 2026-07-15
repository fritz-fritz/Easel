# Wallspan

Wallspan is a greenfield, cross-platform wallpaper manager for complex multi-monitor
desktops. It combines physically correct spanning with a modern Qt 6 interface and a
policy-aware catalog of high-quality, reusable images. Its architecture also distinguishes
scheduled dynamic stills from persistent animated-image and video wallpapers.

This repository currently contains the Stage 1 local still vertical slice: decode/fit/raster,
Compose previews, Qt display enumeration with arrangement persistence, and Plasma 6 / Windows
still apply backends. Use Compose → Open image → Apply on a supported session. CI also captures
apply-payload rasters and Qt GUI smoke screenshots for review. Animated/live media hosts and
additional Linux desktops remain deliberately unimplemented.

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

Wallspan is inspired by the capabilities of Superpaper, but it is not a port and does not
reuse the prior codebase or configuration model.

## Repository layout

```text
apps/wallspan-desktop/     Qt Quick application and CXX-Qt boundary
crates/wallspan-core/      Versioned domain model and validation
crates/wallspan-render/    Display-space planning, raster output, and live frame plans
crates/wallspan-providers/ Online image provider contracts and adapters
crates/wallspan-platform/  Static wallpaper and persistent live-host contracts
docs/                      Product, architecture, provider, and delivery plans
```

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
cargo run -p wallspan-desktop
```

Qt Multimedia becomes a desktop dependency when the live playback stage is implemented; it is
not linked by the current UI-only scaffold.

Distribution package names can change; the CI workflow is the canonical Ubuntu setup.

Read [the product plan](docs/PRODUCT.md), [architecture](docs/ARCHITECTURE.md),
[dynamic and live wallpaper plan](docs/LIVE_WALLPAPERS.md),
[image provider policy](docs/IMAGE_PROVIDERS.md), and [roadmap](docs/ROADMAP.md) before adding
implementation.

## Status and licensing

Wallspan is pre-alpha and private. No distribution license has been selected. Do not copy
code from the reference project into this repository; ideas and observable behavior may be
used as design input.
