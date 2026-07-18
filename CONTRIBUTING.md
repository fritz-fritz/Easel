# Contributing

Thanks for helping improve Easel. By submitting a contribution, you agree to license it under
the Mozilla Public License 2.0, the license covering this repository.

## Engineering rules

1. Keep the domain and renderer independent of Qt and operating-system APIs.
2. Represent logical coordinates, native pixels, and physical dimensions with distinct types.
3. Do not add an online provider until its current terms permit wallpaper use.
4. Never persist provider secrets in profiles, logs, fixtures, screenshots, or CI artifacts.
5. Add deterministic tests for geometry, policy, and serialization behavior.
6. Keep platform mutations behind capability-reporting backend traits.
7. Record consequential design changes as an ADR under `docs/adr`.
8. Do not add packaging, signing, publishing, or storefront credentials to public CI.
9. Do not commit CI visual PNGs or gallery HTML into this repository’s branches. PR galleries
   publish to the separate `easel-ci-visual` Pages repo (see [docs/ci-visual-assets-repo.md](docs/ci-visual-assets-repo.md)).

## Pull-request checks

```sh
cargo fmt --all --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Changes to the QML application must also pass:

```sh
cargo check -p easel-desktop
```

Provider changes must include links to the provider's current official API terms and usage
rules in `docs/IMAGE_PROVIDERS.md`.

Please report security vulnerabilities privately as described in [SECURITY.md](SECURITY.md).
