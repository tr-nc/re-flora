from __future__ import annotations

import subprocess
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
RUNNER = SCRIPTS / "check_ddgi_correctness.sh"


class CheckDdgiCorrectnessTests(unittest.TestCase):
    def test_dry_run_captures_and_validates_each_observable_debug_route(self) -> None:
        result = subprocess.run(
            [str(RUNNER), "--dry-run"],
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        output = result.stdout
        expected_views = (
            "final",
            "moment-visibility",
            "exact-visibility",
            "exact-irradiance",
            "unoccluded-irradiance",
            "equal-weight-irradiance",
            "raw-cage-irradiance",
        )
        for case_name in ("sealed", "portal", "walls"):
            for spacing in (32, 16):
                for view in expected_views:
                    self.assertIn(
                        f"case={case_name} spacing={spacing} backend=ddgi view={view}",
                        output,
                    )

        for view in expected_views:
            self.assertIn(f"--expect-debug-view {view}", output)

        self.assertIn("--min-reference-error-p99", output)
        self.assertIn("--max-reference-error-p99 0.00001", output)
        self.assertIn("--debug-baseline", output)
        self.assertIn("--min-debug-roi-luminance-gain", output)
        self.assertIn("views=8", output)


if __name__ == "__main__":
    unittest.main()
