// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Windows link flags for static libheif / x265.
//!
//! Rust 1.88 stopped auto-linking `advapi32` from libstd on non-win7 targets.
//! Our MSVC static `x265` still calls registry APIs (`RegOpenKeyExA`, etc.), so
//! binaries that only pull in `easel-dynamic` (e.g. examples) need an explicit
//! link.

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
