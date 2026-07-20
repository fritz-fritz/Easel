#!/usr/bin/env python3
"""Keep companion Rust pins aligned with rust-toolchain.toml.

`rust-toolchain.toml` is the single source of truth for the compiler channel.
This script rewrites:

- workspace `rust-version` in the root `Cargo.toml`
- the toolchain mention in `AGENTS.md`

CI installs Rust by reading the same channel (see `.github/actions/setup-rust`).
Dependabot's `rust-toolchain` ecosystem bumps the channel; the
`sync-rust-pins` workflow runs this script on those PRs so companions move
in lockstep.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
TOOLCHAIN_TOML = ROOT / "rust-toolchain.toml"
CARGO_TOML = ROOT / "Cargo.toml"
AGENTS_MD = ROOT / "AGENTS.md"

CHANNEL_RE = re.compile(
    r'(?m)^channel\s*=\s*"(?P<channel>[^"]+)"\s*$',
)
RUST_VERSION_RE = re.compile(
    r'(?m)^(rust-version\s*=\s*")([^"]+)("\s*)$',
)
AGENTS_PIN_RE = re.compile(
    r"(pinned to toolchain `)([^`]+)(` via `rust-toolchain\.toml`)",
)


def read_channel(path: Path) -> str:
    text = path.read_text(encoding="utf-8")
    match = CHANNEL_RE.search(text)
    if not match:
        raise SystemExit(f"could not parse channel from {path}")
    return match.group("channel")


def sync_cargo_toml(channel: str, text: str) -> str:
    if not RUST_VERSION_RE.search(text):
        raise SystemExit(f"could not find rust-version in {CARGO_TOML}")
    return RUST_VERSION_RE.sub(rf"\g<1>{channel}\g<3>", text, count=1)


def sync_agents_md(channel: str, text: str) -> str:
    if not AGENTS_PIN_RE.search(text):
        raise SystemExit(f"could not find toolchain pin sentence in {AGENTS_MD}")
    return AGENTS_PIN_RE.sub(rf"\g<1>{channel}\g<3>", text, count=1)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="exit 1 when companion pins drift (do not write)",
    )
    args = parser.parse_args()

    channel = read_channel(TOOLCHAIN_TOML)
    cargo_text = CARGO_TOML.read_text(encoding="utf-8")
    agents_text = AGENTS_MD.read_text(encoding="utf-8")

    new_cargo = sync_cargo_toml(channel, cargo_text)
    new_agents = sync_agents_md(channel, agents_text)

    drifted = (new_cargo != cargo_text) or (new_agents != agents_text)
    if args.check:
        if drifted:
            print(
                "Rust companion pins are out of sync with rust-toolchain.toml.\n"
                f"  channel: {channel}\n"
                "Run: python3 .github/scripts/sync-rust-pins.py",
                file=sys.stderr,
            )
            return 1
        print(f"ok: companion pins match channel {channel}")
        return 0

    if not drifted:
        print(f"unchanged: companion pins already match channel {channel}")
        return 0

    CARGO_TOML.write_text(new_cargo, encoding="utf-8")
    AGENTS_MD.write_text(new_agents, encoding="utf-8")
    print(f"updated companion pins to channel {channel}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
