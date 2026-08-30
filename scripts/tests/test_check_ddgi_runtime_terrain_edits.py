from __future__ import annotations

import subprocess
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
RUNNER = SCRIPTS / "check_ddgi_runtime_terrain_edits.sh"


class CheckDdgiRuntimeTerrainEditsTests(unittest.TestCase):
    def test_dry_run_proves_owner_history_on_both_real_local_recoveries(self) -> None:
        result = subprocess.run(
            [str(RUNNER), "--dry-run"],
            check=False,
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        output = result.stdout
        for spacing in (32, 16):
            evidence_capture = (
                f"sequential-reopened-spacing{spacing}-final-a.rfirr"
            )
            evidence_lines = [
                line
                for line in output.splitlines()
                if evidence_capture in line
                and "analyze_environment_irradiance_capture.py" in line
            ]
            self.assertEqual(len(evidence_lines), 1, evidence_lines)
            self.assertIn("--expect-version 9", evidence_lines[0])
            self.assertIn(
                "--require-filter-history-retain-blend", evidence_lines[0]
            )
            self.assertIn(
                "--require-filter-local-recovery-policy", evidence_lines[0]
            )
            self.assertNotIn("--expect-filter-blend-retention-q16", evidence_lines[0])


if __name__ == "__main__":
    unittest.main()
