# Contributing

Wallspan is currently developed in a private repository.

## Engineering rules

1. Keep the domain and renderer independent of Qt and operating-system APIs.
2. Represent logical coordinates, native pixels, and physical dimensions with distinct types.
3. Do not add an online provider until its current terms permit wallpaper use.
4. Never persist provider secrets in profiles, logs, fixtures, or screenshots.
5. Add deterministic tests for geometry, policy, and serialization behavior.
6. Keep platform mutations behind capability-reporting backend traits.
7. Record consequential design changes as an ADR under `docs/adr`.

## Pull-request checks

```sh
cargo fmt --all --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Changes to the QML application must also pass:

```sh
cargo check -p wallspan-desktop
```

Provider changes must include links to the provider's current official API terms and usage
rules in `docs/IMAGE_PROVIDERS.md`.
