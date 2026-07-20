#!/usr/bin/env python3
"""Unit tests for sync-rust-pins.py."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("sync-rust-pins.py")


def load_module():
    spec = importlib.util.spec_from_file_location("sync_rust_pins", SCRIPT)
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class SyncRustPinsTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.mod = load_module()

    def test_read_channel(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "rust-toolchain.toml"
            path.write_text('[toolchain]\nchannel = "1.97"\n', encoding="utf-8")
            self.assertEqual(self.mod.read_channel(path), "1.97")

    def test_sync_rewrites_companions(self) -> None:
        cargo = '[workspace.package]\nrust-version = "1.88"\n'
        agents = "pinned to toolchain `1.88` via `rust-toolchain.toml`\n"
        self.assertEqual(
            self.mod.sync_cargo_toml("1.97", cargo),
            '[workspace.package]\nrust-version = "1.97"\n',
        )
        self.assertEqual(
            self.mod.sync_agents_md("1.97", agents),
            "pinned to toolchain `1.97` via `rust-toolchain.toml`\n",
        )


if __name__ == "__main__":
    unittest.main()
