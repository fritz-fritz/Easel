#!/usr/bin/env python3
"""Ensure cxx and cxx-gen patch versions stay aligned.

cxx-qt-build generates C++ bridges via `cxx-gen`, while Rust `#[cxx::bridge]`
macros come from the `cxx` crate. Both embed `CARGO_PKG_VERSION_PATCH` in
symbol names (`cxxbridge1$N$…`). If Dependabot bumps `cxx` (1.0.N) without
also bumping transitive `cxx-gen` (0.7.N), desktop linking fails with
unresolved `$197$` vs `$198$` symbols.

When this check fails, run:
  cargo update -p cxx-gen --precise 0.7.<cxx-patch>
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


LOCK = Path(__file__).resolve().parents[2] / "Cargo.lock"
PACKAGE_RE = re.compile(
    r'(?ms)^\[\[package\]\]\nname = "(?P<name>[^"]+)"\nversion = "(?P<version>[^"]+)"'
)


def package_versions(lock_text: str) -> dict[str, str]:
    return {m.group("name"): m.group("version") for m in PACKAGE_RE.finditer(lock_text)}


def patch_component(version: str) -> str:
    parts = version.split(".")
    if len(parts) < 3:
        raise ValueError(f"expected major.minor.patch, got {version!r}")
    return parts[2].split("+", 1)[0]


def main() -> int:
    versions = package_versions(LOCK.read_text(encoding="utf-8"))
    try:
        cxx = versions["cxx"]
        cxx_gen = versions["cxx-gen"]
    except KeyError as missing:
        print(f"Cargo.lock missing package {missing.args[0]!r}", file=sys.stderr)
        return 1

    cxx_patch = patch_component(cxx)
    gen_patch = patch_component(cxx_gen)
    if not cxx.startswith("1.0.") or not cxx_gen.startswith("0.7."):
        print(
            f"unexpected version scheme: cxx={cxx} cxx-gen={cxx_gen}",
            file=sys.stderr,
        )
        return 1
    if cxx_patch != gen_patch:
        print(
            "cxx / cxx-gen patch mismatch "
            f"(cxx={cxx}, cxx-gen={cxx_gen}).\n"
            "cxx-qt C++ codegen and Rust cxx macros embed different symbol "
            "version suffixes and desktop will fail to link.\n"
            f"Fix: cargo update -p cxx-gen --precise 0.7.{cxx_patch}",
            file=sys.stderr,
        )
        return 1

    print(f"ok: cxx {cxx} aligns with cxx-gen {cxx_gen}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
