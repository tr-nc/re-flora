from __future__ import annotations

import sys
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
DIAGNOSTICS = SCRIPTS / "runtime_log_diagnostics.py"
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

    def test_cli_accepts_exact_wayland_portal_timeout_across_multiple_logs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app_log = root / "app.log"
            run_log = root / "run.log"
            app_log.write_text(BENIGN_PORTAL_TIMEOUT + "\n")
            run_log.write_text("validation layers enabled\nerrors=0\n")

            result = subprocess.run(
                [sys.executable, str(DIAGNOSTICS), str(app_log), str(run_log)],
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")

    def test_cli_reports_a_near_miss_from_the_second_log(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app_log = root / "app.log"
            run_log = root / "run.log"
            app_log.write_text("clean\n")
            near_miss = BENIGN_PORTAL_TIMEOUT.replace("100ms", "101ms")
            run_log.write_text(near_miss + "\n")

            result = subprocess.run(
                [sys.executable, str(DIAGNOSTICS), str(app_log), str(run_log)],
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertEqual(result.returncode, 1)
        self.assertIn(str(run_log), result.stdout)
        self.assertIn(near_miss, result.stdout)

    def test_cli_fails_closed_when_any_log_cannot_be_read(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            missing = Path(directory) / "missing.log"
            result = subprocess.run(
                [sys.executable, str(DIAGNOSTICS), str(missing)],
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertEqual(result.returncode, 2)
        self.assertIn("reason=log-read-failed", result.stderr)
        self.assertIn(str(missing), result.stderr)
