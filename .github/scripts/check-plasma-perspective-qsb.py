#!/usr/bin/env python3
# Copyright (c) The Easel Authors.
# SPDX-License-Identifier: MPL-2.0
"""Fail CI when the Plasma projective live shader pack is missing, empty, or stale.

The wallpaper plugin samples live perspective via ShaderEffect + a checked-in
`.qsb` (ADR 0016). install.sh and PlasmaLiveBackend both require this asset.

When `qsb` is available, rebuilds into a temp file and compares a canonical
`qsb -d` dump (sorted targets + reflection JSON). Raw `.qsb` bytes are not
stable across qsb runs (unordered packs / compression), so byte equality is
not used.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SHADER_DIR = (
    REPO_ROOT / "apps" / "easel-plasma-wallpaper" / "contents" / "ui" / "shaders"
)
FRAG = SHADER_DIR / "perspective.frag"
QSB = SHADER_DIR / "perspective.frag.qsb"
MIN_BYTES = 256
QSB_ARGS = ["--glsl", "100 es,120,150", "--hlsl", "50", "--msl", "12"]
SHADER_LINE = re.compile(r"Shader \d+: (.+) \[Standard\]")


def find_qsb() -> Path | None:
    env = os.environ.get("QSB")
    if env:
        path = Path(env)
        if path.is_file() and os.access(path, os.X_OK):
            return path
    which = shutil.which("qsb")
    if which:
        return Path(which)
    candidate = Path("/usr/lib/qt6/bin/qsb")
    if candidate.is_file() and os.access(candidate, os.X_OK):
        return candidate
    return None


def canonical_dump(qsb_bin: Path, pack: Path) -> tuple[list[str], dict]:
    proc = subprocess.run(
        [str(qsb_bin), "-d", str(pack)],
        check=True,
        capture_output=True,
        text=True,
    )
    text = proc.stdout + proc.stderr
    shaders = sorted(SHADER_LINE.findall(text))
    marker = "Reflection info:"
    idx = text.find(marker)
    if idx < 0:
        raise RuntimeError(f"no reflection info in qsb dump for {pack}")
    json_text = text[idx + len(marker) :].strip()
    # Dump may append trailing notes; decode the first JSON object.
    decoder = json.JSONDecoder()
    reflection, _ = decoder.raw_decode(json_text)
    return shaders, reflection


def freshness_errors(qsb_bin: Path) -> list[str]:
    errors: list[str] = []
    with tempfile.TemporaryDirectory(prefix="easel-qsb-") as tmp:
        out = Path(tmp) / "perspective.frag.qsb"
        cmd = [str(qsb_bin), *QSB_ARGS, "-o", str(out), str(FRAG)]
        try:
            subprocess.run(cmd, check=True, capture_output=True, text=True)
        except subprocess.CalledProcessError as exc:
            errors.append(
                "qsb rebuild failed:\n"
                f"  cmd: {' '.join(cmd)}\n"
                f"  stdout: {exc.stdout}\n"
                f"  stderr: {exc.stderr}"
            )
            return errors
        if not out.is_file() or out.stat().st_size < MIN_BYTES:
            errors.append(f"qsb rebuild produced empty/missing output: {out}")
            return errors
        try:
            checked_shaders, checked_reflect = canonical_dump(qsb_bin, QSB)
            rebuilt_shaders, rebuilt_reflect = canonical_dump(qsb_bin, out)
        except (subprocess.CalledProcessError, RuntimeError, json.JSONDecodeError) as exc:
            errors.append(f"qsb dump compare failed: {exc}")
            return errors
        if checked_shaders != rebuilt_shaders:
            errors.append(
                "checked-in perspective.frag.qsb shader targets differ from a fresh "
                f"rebuild: {checked_shaders} vs {rebuilt_shaders} — run "
                "tools/dev/compile-plasma-shaders.sh"
            )
        if checked_reflect != rebuilt_reflect:
            errors.append(
                "checked-in perspective.frag.qsb reflection info differs from a fresh "
                "rebuild — fragment uniforms/samplers changed; run "
                "tools/dev/compile-plasma-shaders.sh and commit the updated .qsb"
            )
    return errors


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

    if not errors:
        qsb_bin = find_qsb()
        if qsb_bin is not None:
            errors.extend(freshness_errors(qsb_bin))
            if not errors:
                print(
                    f"ok: {QSB.relative_to(REPO_ROOT)} ({QSB.stat().st_size} bytes); "
                    f"canonical dump matches rebuild via {qsb_bin}"
                )
                return 0
        else:
            print(
                f"ok: {QSB.relative_to(REPO_ROOT)} ({QSB.stat().st_size} bytes); "
                "qsb not found — skipped freshness rebuild compare"
            )
            return 0

    for err in errors:
        print(err, file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
