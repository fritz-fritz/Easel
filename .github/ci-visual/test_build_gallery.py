#!/usr/bin/env python3
# Copyright (c) contributors. MPL-2.0.
"""Unit tests for build_gallery.py (stdlib unittest, no pytest required)."""

from __future__ import annotations

import json
import struct
import tempfile
import unittest
import zlib
from pathlib import Path

from build_gallery import (
    build_comparisons,
    collect_images,
    comparison_totals,
    decode_png_rgba,
    extract_display_label,
    load_manifests,
    main,
    png_meta,
    raw_asset_base,
    safe_filename,
)


def write_png(path: Path, width: int, height: int, rgb: tuple[int, int, int]) -> None:
    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    raw = b"".join(b"\x00" + bytes(rgb) * width for _ in range(height))
    data = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    path.write_bytes(data)


def write_manifest(root: Path, stage: str, os_name: str, filenames: list[str]) -> None:
    payload = {
        "stage": stage,
        "os": os_name,
        "sha": "deadbeef",
        "run_id": "1",
        "run_attempt": "1",
        "repository": "fritz-fritz/Easel",
        "bundle": f"ci-visual-{stage}-{os_name}",
        "images": [
            {
                "filename": name,
                "stem": Path(name).stem.split(f"{stage}-{os_name}-", 1)[-1],
            }
            for name in filenames
        ],
    }
    (root / f"ci-visual-manifest-{stage}-{os_name}.json").write_text(
        json.dumps(payload), encoding="utf8"
    )


class BuildGalleryTests(unittest.TestCase):
    def test_png_meta_and_raw_base(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "a.png"
            write_png(path, 32, 18, (10, 20, 30))
            meta = png_meta(path)
            self.assertEqual(meta["width"], 32)
            self.assertEqual(meta["height"], 18)
            self.assertEqual(len(meta["sha256"]), 64)
        self.assertEqual(
            raw_asset_base("https://fritz-fritz.github.io/easel-ci-visual"),
            "https://raw.githubusercontent.com/fritz-fritz/easel-ci-visual/gh-pages",
        )
        self.assertEqual(extract_display_label("apply-display-2"), "2")

    def test_safe_filename_rejects_path_separators(self) -> None:
        self.assertEqual(safe_filename("ok.png"), "ok.png")
        with self.assertRaises(ValueError):
            safe_filename("subdir/foo.png")
        with self.assertRaises(ValueError):
            safe_filename("subdir\\foo.png")

    def test_strict_mismatch_fails_gate(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for os_name, rgb in [
                ("ubuntu-latest", (1, 2, 3)),
                ("windows-latest", (10, 20, 30)),  # far above ±1 LSB tolerance
                ("macos-latest", (1, 2, 3)),
            ]:
                name = f"apply-payload-{os_name}-apply-display-0.png"
                write_png(root / name, 16, 9, rgb)
                write_manifest(root, "apply-payload", os_name, [name])
            images = collect_images(root, load_manifests(root))
            comparisons = build_comparisons(images)
            totals = comparison_totals(comparisons)
            self.assertEqual(len(comparisons), 1)
            self.assertEqual(comparisons[0]["status"], "content-mismatch")
            self.assertFalse(totals["gate_ok"])
            self.assertEqual(totals["strict_match"], 0)

    def test_strict_one_lsb_is_tolerant_match(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            width, height = 8, 4
            pixels = bytearray([128, 255, 0, 255] * (width * height))

            def encode_rgba(buf: bytes) -> bytes:
                raw = b"".join(
                    b"\x00" + buf[y * width * 4 : (y + 1) * width * 4]
                    for y in range(height)
                )

                def chunk(tag: bytes, data: bytes) -> bytes:
                    return (
                        struct.pack(">I", len(data))
                        + tag
                        + data
                        + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
                    )

                return (
                    b"\x89PNG\r\n\x1a\n"
                    + chunk(
                        b"IHDR",
                        struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0),
                    )
                    + chunk(b"IDAT", zlib.compress(raw, 9))
                    + chunk(b"IEND", b"")
                )

            base_blob = encode_rgba(bytes(pixels))
            mutated = bytearray(pixels)
            mutated[0] = (mutated[0] - 1) & 0xFF
            alt_blob = encode_rgba(bytes(mutated))
            self.assertIsNotNone(decode_png_rgba)
            for os_name, blob in [
                ("ubuntu-latest", base_blob),
                ("macos-latest", base_blob),
                ("windows-latest", alt_blob),
            ]:
                name = f"apply-payload-{os_name}-apply-display-0.png"
                (root / name).write_bytes(blob)
                write_manifest(root, "apply-payload", os_name, [name])
            comparisons = build_comparisons(collect_images(root, load_manifests(root)))
            totals = comparison_totals(comparisons)
            self.assertEqual(comparisons[0]["status"], "match-tolerant")
            self.assertTrue(totals["gate_ok"])
            self.assertEqual(totals["strict_match"], 1)
            self.assertEqual(comparisons[0]["pixel_compare"]["max_channel_delta"], 1)

    def test_display_numeric_sort_order(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            # Insert out of numeric order so lexicographic sort would put 10 before 2.
            for display in ("10", "2", "0"):
                for os_name in ("ubuntu-latest", "windows-latest", "macos-latest"):
                    name = f"apply-payload-{os_name}-apply-display-{display}.png"
                    write_png(root / name, 8, 8, (1, 2, 3))
            for os_name in ("ubuntu-latest", "windows-latest", "macos-latest"):
                write_manifest(
                    root,
                    "apply-payload",
                    os_name,
                    [
                        f"apply-payload-{os_name}-apply-display-10.png",
                        f"apply-payload-{os_name}-apply-display-2.png",
                        f"apply-payload-{os_name}-apply-display-0.png",
                    ],
                )
            comparisons = build_comparisons(collect_images(root, load_manifests(root)))
            self.assertEqual([c["asset"] for c in comparisons], ["0", "2", "10"])

    def test_strict_match_excludes_incomplete_matrix(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            # Only ubuntu produces the asset → single-os warning, not a full match.
            name = "apply-payload-ubuntu-latest-apply-display-0.png"
            write_png(root / name, 8, 8, (1, 2, 3))
            write_manifest(root, "apply-payload", "ubuntu-latest", [name])
            totals = comparison_totals(
                build_comparisons(collect_images(root, load_manifests(root)))
            )
            self.assertTrue(totals["gate_ok"])
            self.assertEqual(totals["strict_match"], 0)
            self.assertEqual(totals["strict_warnings"], 1)
            self.assertEqual(totals["strict_ok"], 1)

    def test_strict_match_and_gui_variance(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            root = base / "artifacts"
            out = base / "out"
            root.mkdir()
            for os_name in ("ubuntu-latest", "windows-latest", "macos-latest"):
                apply_name = f"apply-payload-{os_name}-apply-display-0.png"
                gui_name = f"gui-smoke-{os_name}-gui.png"
                write_png(root / apply_name, 16, 9, (9, 9, 9))
                write_png(
                    root / gui_name,
                    40,
                    20,
                    (10, 10, 10) if os_name == "ubuntu-latest" else (11, 10, 10),
                )
                write_manifest(root, "apply-payload", os_name, [apply_name])
                write_manifest(root, "gui-smoke", os_name, [gui_name])

            rc = main_with(
                artifacts=root,
                out=out,
                fail_on_mismatch=True,
            )
            self.assertEqual(rc, 0)
            summary = json.loads((out / "summary.json").read_text(encoding="utf8"))
            self.assertTrue(summary["comparison"]["gate_ok"])
            self.assertEqual(summary["comparison"]["strict_match"], 1)
            self.assertGreaterEqual(summary["comparison"]["informational_variance"], 1)
            comment = (out / "comment.md").read_text(encoding="utf8")
            self.assertIn("Apply-payload OS compare", comment)
            self.assertIn("fully identical across runners", comment)
            self.assertIn("SHA-256 (12)", comment)
            self.assertIn("Cross-OS metadata", comment)
            html_page = (out / "site" / "index.html").read_text(encoding="utf8")
            self.assertIn("OS compare", html_page)
            self.assertIn("fully OS-identical", html_page)
            self.assertIn('table class="compare"', html_page)

    def test_fail_on_mismatch_exit_code(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            root = base / "artifacts"
            out = base / "out"
            root.mkdir()
            for os_name, rgb in [
                ("ubuntu-latest", (1, 2, 3)),
                ("windows-latest", (9, 9, 9)),
                ("macos-latest", (1, 2, 3)),
            ]:
                name = f"apply-payload-{os_name}-apply-display-0.png"
                write_png(root / name, 8, 8, rgb)
                write_manifest(root, "apply-payload", os_name, [name])
            self.assertEqual(main_with(root, out, fail_on_mismatch=True), 1)
            self.assertEqual(main_with(root, out, fail_on_mismatch=False), 0)


def main_with(artifacts: Path, out: Path, *, fail_on_mismatch: bool) -> int:
    argv = [
        "build_gallery.py",
        "--artifacts",
        str(artifacts),
        "--out",
        str(out),
        "--pr-number",
        "42",
        "--sha",
        "abcdef0123456789",
        "--run-url",
        "https://example.test/run/1",
        "--run-id",
        "1",
        "--deployed",
    ]
    argv.append("--fail-on-os-mismatch" if fail_on_mismatch else "--no-fail-on-os-mismatch")
    import sys

    old = sys.argv
    try:
        sys.argv = argv
        return main()
    finally:
        sys.argv = old


if __name__ == "__main__":
    raise SystemExit(unittest.main())
