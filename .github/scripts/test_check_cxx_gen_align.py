#!/usr/bin/env python3
"""Unit tests for check-cxx-gen-align.py."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check-cxx-gen-align.py")


def load_module():
    spec = importlib.util.spec_from_file_location("check_cxx_gen_align", SCRIPT)
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class CheckCxxGenAlignTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.mod = load_module()

    def test_aligned_versions(self) -> None:
        text = (
            '[[package]]\nname = "cxx"\nversion = "1.0.198"\n\n'
            '[[package]]\nname = "cxx-gen"\nversion = "0.7.198"\n'
        )
        versions = self.mod.package_versions(text)
        self.assertEqual(self.mod.patch_component(versions["cxx"]), "198")
        self.assertEqual(self.mod.patch_component(versions["cxx-gen"]), "198")

    def test_mismatch_detected(self) -> None:
        text = (
            '[[package]]\nname = "cxx"\nversion = "1.0.198"\n\n'
            '[[package]]\nname = "cxx-gen"\nversion = "0.7.197"\n'
        )
        versions = self.mod.package_versions(text)
        self.assertNotEqual(
            self.mod.patch_component(versions["cxx"]),
            self.mod.patch_component(versions["cxx-gen"]),
        )


if __name__ == "__main__":
    unittest.main()
