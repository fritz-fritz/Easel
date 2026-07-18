#!/usr/bin/env python3
# Copyright (c) contributors. MPL-2.0.
"""Select easel-desktop smoke screenshot views from a changed-file list.

Prints a comma-separated view list for `--smoke-views` (stdout). Always includes
the fixture `preview` capture so the mocked multi-monitor layout stays in CI.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

VIEW_ORDER = (
    "preview",
    "compose",
    "discover",
    "library",
    "profiles",
    "automation",
)

# Paths that affect chrome / shared controllers / core render → all GUI pages.
SHARED_PREFIXES = (
    "apps/easel-desktop/qml/main.qml",
    "apps/easel-desktop/src/app_controller.rs",
    "apps/easel-desktop/src/main.rs",
    "apps/easel-desktop/src/display_session.rs",
    "apps/easel-desktop/src/fixtures.rs",
    "apps/easel-desktop/build.rs",
    "apps/easel-desktop/Cargo.toml",
    "apps/easel-desktop/assets/",
    "crates/easel-core/",
    "crates/easel-render/",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".github/actions/ci-visual/",
    ".github/ci-visual/",
    ".github/workflows/ci.yml",
)

VIEW_PREFIXES: dict[str, tuple[str, ...]] = {
    "preview": (
        "apps/easel-desktop/qml/components/MonitorPreview.qml",
        "apps/easel-desktop/src/fixtures.rs",
        "apps/easel-desktop/assets/",
        "apps/easel-desktop/src/display_session.rs",
        "apps/easel-desktop/src/compose_controller.rs",
    ),
    "compose": (
        "apps/easel-desktop/src/compose_controller.rs",
        "apps/easel-desktop/src/apply_service.rs",
        "apps/easel-desktop/qml/components/MonitorPreview.qml",
        "crates/easel-platform/",
    ),
    "discover": (
        "apps/easel-desktop/src/discover_controller.rs",
        "apps/easel-desktop/qml/components/PhotoCard.qml",
        "crates/easel-providers/",
    ),
    "library": (
        "apps/easel-desktop/src/library_controller.rs",
        "apps/easel-desktop/src/library_session.rs",
        "apps/easel-desktop/qml/components/PhotoCard.qml",
        "crates/easel-library/",
    ),
    "profiles": (
        "apps/easel-desktop/src/profile_controller.rs",
        "crates/easel-scheduler/",
    ),
    "automation": (
        "apps/easel-desktop/src/automation_controller.rs",
        "apps/easel-desktop/src/automation_session.rs",
        "crates/easel-scheduler/",
        "apps/easel-cli/",
    ),
}


def normalize_path(path: str) -> str:
    return path.strip().replace("\\", "/").lstrip("./")


def path_matches(path: str, prefix: str) -> bool:
    path = normalize_path(path)
    prefix = normalize_path(prefix)
    if prefix.endswith("/"):
        return path.startswith(prefix) or path == prefix.rstrip("/")
    return path == prefix or path.startswith(prefix + "/")


def is_shared(path: str) -> bool:
    return any(path_matches(path, prefix) for prefix in SHARED_PREFIXES)


def select_smoke_views(paths: list[str]) -> list[str]:
    """Return ordered smoke view ids for the given changed paths."""
    normalized = [normalize_path(path) for path in paths if path.strip()]
    selected: set[str] = {"preview"}

    if not normalized:
        selected.add("compose")
        return [view for view in VIEW_ORDER if view in selected]

    if any(is_shared(path) for path in normalized):
        return list(VIEW_ORDER)

    for path in normalized:
        for view, prefixes in VIEW_PREFIXES.items():
            if any(path_matches(path, prefix) for prefix in prefixes):
                selected.add(view)

    # Always pair the fixture preview with at least one full-window GUI capture.
    if selected == {"preview"}:
        selected.add("compose")

    return [view for view in VIEW_ORDER if view in selected]


def read_paths(args: argparse.Namespace) -> list[str]:
    if args.paths_file:
        text = Path(args.paths_file).read_text(encoding="utf8")
        return [line for line in text.splitlines() if line.strip()]
    if args.paths:
        return list(args.paths)
    return [line for line in sys.stdin.read().splitlines() if line.strip()]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "paths",
        nargs="*",
        help="Changed file paths (or pass via --paths-file / stdin)",
    )
    parser.add_argument(
        "--paths-file",
        help="Newline-separated changed paths",
    )
    args = parser.parse_args(argv)
    views = select_smoke_views(read_paths(args))
    print(",".join(views))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
