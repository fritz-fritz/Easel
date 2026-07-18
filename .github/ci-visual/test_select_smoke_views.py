#!/usr/bin/env python3
# Copyright (c) contributors. MPL-2.0.
"""Unit tests for select_smoke_views.py."""

from __future__ import annotations

import unittest

from select_smoke_views import VIEW_ORDER, select_smoke_views


class SelectSmokeViewsTests(unittest.TestCase):
    def test_empty_defaults_to_preview_and_compose(self) -> None:
        self.assertEqual(select_smoke_views([]), ["preview", "compose"])

    def test_shared_shell_selects_all(self) -> None:
        self.assertEqual(
            select_smoke_views(["apps/easel-desktop/qml/main.qml"]),
            list(VIEW_ORDER),
        )
        self.assertEqual(
            select_smoke_views(["crates/easel-core/src/lib.rs"]),
            list(VIEW_ORDER),
        )

    def test_provider_crate_selects_discover(self) -> None:
        self.assertEqual(
            select_smoke_views(["crates/easel-providers/src/lib.rs"]),
            ["preview", "discover"],
        )

    def test_discover_only(self) -> None:
        self.assertEqual(
            select_smoke_views(["apps/easel-desktop/src/discover_controller.rs"]),
            ["preview", "discover"],
        )

    def test_library_and_automation(self) -> None:
        self.assertEqual(
            select_smoke_views(
                [
                    "apps/easel-desktop/src/library_session.rs",
                    "apps/easel-desktop/src/automation_controller.rs",
                ]
            ),
            ["preview", "library", "automation"],
        )

    def test_unrelated_path_still_gets_compose_gui(self) -> None:
        self.assertEqual(
            select_smoke_views(["docs/QUALITY.md"]),
            ["preview", "compose"],
        )

    def test_photo_card_maps_discover_and_library(self) -> None:
        self.assertEqual(
            select_smoke_views(["apps/easel-desktop/qml/components/PhotoCard.qml"]),
            ["preview", "discover", "library"],
        )

    def test_monitor_preview_maps_preview_and_compose(self) -> None:
        self.assertEqual(
            select_smoke_views(
                ["apps/easel-desktop/qml/components/MonitorPreview.qml"]
            ),
            ["preview", "compose"],
        )


if __name__ == "__main__":
    unittest.main()
