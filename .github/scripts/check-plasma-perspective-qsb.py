#!/usr/bin/env python3
# Copyright (c) The Easel Authors.
# SPDX-License-Identifier: MPL-2.0
"""Fail CI when the Plasma projective live shader pack is missing or empty.

The wallpaper plugin samples live perspective via ShaderEffect + a checked-in
`.qsb` (ADR 0016). install.sh and PlasmaLiveBackend both require this asset.
"""

from __future__ import annotations

import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SHADER_DIR = (
    REPO_ROOT / "apps" / "easel-plasma-wallpaper" / "contents" / "ui" / "shaders"
)
FRAG = SHADER_DIR / "perspective.frag"
QSB = SHADER_DIR / "perspective.frag.qsb"
MIN_BYTES = 256


def main() -> int:
    errors: list[str] = []
    if not FRAG.is_file():
        errors.append(f"missing fragment source: {FRAG.relative_to(REPO_ROOT)}")
    if not QSB.is_file():
        errors.append(f"missing shader pack: {QSB.relative_to(REPO_ROOT)}")
    elif QSB.stat().st_size < MIN_BYTES:
        errors.append(
            f"shader pack too small ({QSB.stat().st_size} bytes): "
            f"{QSB.relative_to(REPO_ROOT)} — run tools/dev/compile-plasma-shaders.sh"
        )

    if errors:
        for err in errors:
            print(err, file=sys.stderr)
        return 1
    print(f"ok: {QSB.relative_to(REPO_ROOT)} ({QSB.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
