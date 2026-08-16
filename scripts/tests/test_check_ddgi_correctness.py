from __future__ import annotations

import subprocess
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
RUNNER = SCRIPTS / "check_ddgi_correctness.sh"


class CheckDdgiCorrectnessTests(unittest.TestCase):
    def test_dry_run_captures_only_terminal_temporal_fields(self) -> None:
        result = subprocess.run(
            [str(RUNNER), "--dry-run"],
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            result.stdout.count("--environment-irradiance-capture-target converged"),
            30,
        )
        self.assertIn("dry-run matrix cases=3 spacings=2 views=5", result.stdout)

    def test_moment_only_walls_keep_calibrated_exact_reference_ceilings(self) -> None:
        source = RUNNER.read_text()

        self.assertIn("--max-reference-error-p99 0.40", source)
        self.assertIn("--max-reference-error-p99 0.375", source)
        self.assertIn("Runtime consumers intentionally use Moment visibility only", source)


if __name__ == "__main__":
    unittest.main()
