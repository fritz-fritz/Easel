# Distribution policy

## Source and community builds

Easel source is available under the Mozilla Public License 2.0. The license permits forks,
modification, and redistribution when its conditions are followed. Community-built binaries must
make the corresponding source available as required by the MPL and must not present themselves as
official Easel packages.

## Official builds

Official packages are produced by the Easel maintainers, signed with controlled platform
identities, and delivered through designated storefronts. A purchase pays for convenient
installation, a trusted update channel, and project support; it does not remove the source-code
rights granted by the MPL.

The official designation is established by the storefront publisher, signing identity, and update
channel—not by the fact that a binary was compiled from this repository. See
[the name and branding guidance](../TRADEMARKS.md).

## Public CI boundary

GitHub Actions may build and test the source. Public workflow artifacts are limited to
non-installable visual test output such as synthetic apply-payload rasters and GUI smoke
screenshots.

Public workflows must not upload application executables, installers, bundles, package archives,
symbols containing sensitive paths, signing material, notarization credentials, storefront API
keys, or update-channel credentials. Packaging and storefront delivery use a separately controlled
release process that transfers packages directly to the relevant storefront rather than through
public GitHub artifacts.
