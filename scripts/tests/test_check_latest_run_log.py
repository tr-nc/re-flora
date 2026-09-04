from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
CHECKER = SCRIPTS / "check_latest_run_log.py"
BENIGN_PORTAL_TIMEOUT = (
    "[12:34:56.789 ERROR sctk_adwaita::config] "
    "XDG Settings Portal did not return response in time: "
    "timeout: 100ms, key: color-scheme\n"
)


class CheckLatestRunLogTests(unittest.TestCase):
    def run_checker(self, log_text: str) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = root / "run.log"
            log.write_text(log_text, encoding="utf-8")
            (root / "latest-run-log.txt").write_text(str(log), encoding="utf-8")
            return subprocess.run(
                ["python3", str(CHECKER), "--log-root", str(root)],
                check=False,
                capture_output=True,
                text=True,
            )

    def test_accepts_the_exact_cosmetic_portal_timeout(self) -> None:
        result = self.run_checker(BENIGN_PORTAL_TIMEOUT + "clean completion\n")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_portal_near_miss_and_cross_line_fatal(self) -> None:
        for diagnostic in (
            BENIGN_PORTAL_TIMEOUT.replace(" ERROR ", " WARN "),
            BENIGN_PORTAL_TIMEOUT.replace("100ms", "101ms"),
            "device\nlost\n",
        ):
            with self.subTest(diagnostic=diagnostic):
                result = self.run_checker(diagnostic)
                self.assertEqual(result.returncode, 1)
                self.assertIn("fatal or validation", result.stderr)


if __name__ == "__main__":
    unittest.main()
