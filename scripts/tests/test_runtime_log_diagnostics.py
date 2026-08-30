from __future__ import annotations

import sys
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

from runtime_log_diagnostics import (  # noqa: E402
    fatal_diagnostic_excerpts,
    first_fatal_diagnostic,
)


BENIGN_PORTAL_TIMEOUT = (
    "[12:34:56.789 ERROR sctk_adwaita::config] "
    "XDG Settings Portal did not return response in time: "
    "timeout: 100ms, key: color-scheme"
)


class RuntimeLogDiagnosticsTests(unittest.TestCase):
    def test_accepts_only_the_exact_cosmetic_portal_timeout(self) -> None:
        self.assertIsNone(first_fatal_diagnostic(BENIGN_PORTAL_TIMEOUT))

    def test_portal_near_miss_and_independent_fatal_are_not_masked(self) -> None:
        for diagnostic in (
            BENIGN_PORTAL_TIMEOUT.replace(" ERROR ", " WARN "),
            BENIGN_PORTAL_TIMEOUT.replace("100ms", "101ms"),
            BENIGN_PORTAL_TIMEOUT + " VUID-injected",
            BENIGN_PORTAL_TIMEOUT + "\nERROR independent",
        ):
            with self.subTest(diagnostic=diagnostic):
                self.assertIsNotNone(first_fatal_diagnostic(diagnostic))

    def test_preserves_cross_line_fatal_diagnostics(self) -> None:
        for diagnostic in ("device\nlost", "destroyed\ndescriptor", "stale\nreadback"):
            with self.subTest(diagnostic=diagnostic):
                self.assertIsNotNone(first_fatal_diagnostic(diagnostic))
                self.assertEqual(fatal_diagnostic_excerpts(diagnostic), [diagnostic])
