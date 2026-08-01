from __future__ import annotations

import subprocess
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
RUNNER = SCRIPTS / "check_ddgi_transport_acceptance.sh"


class CheckDdgiTransportAcceptanceTests(unittest.TestCase):
    def run_runner(
        self, *arguments: str
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(RUNNER), *arguments],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_dry_run_exposes_the_required_transport_matrix(self) -> None:
        result = self.run_runner("--dry-run")

        self.assertEqual(result.returncode, 0, result.stderr)
        output = result.stdout
        for spacing in (32, 16):
            for stage in ("s0", "s1", "s2", "converged"):
                self.assertIn(
                    f"case=sealed spacing={spacing} target={stage} order=forward",
                    output,
                )
            for stage in ("s0", "s1"):
                self.assertIn(
                    f"case=donor spacing={spacing} target={stage} order=forward",
                    output,
                )
            for stage in ("s1", "s2"):
                self.assertIn(
                    f"case=dogleg spacing={spacing} target={stage} order=forward",
                    output,
                )
            for case_name in ("portal", "donor", "dogleg"):
                self.assertIn(
                    f"case={case_name} spacing={spacing} target=converged order=forward",
                    output,
                )
            self.assertIn(
                f"case=donor spacing={spacing} target=s1 order=reverse", output
            )

        for contract in (
            "--require-zero-rgb",
            "--expect-transport-stage seed-sky",
            "--expect-publication-state unpublished",
            "--expect-batch-order reverse",
            "--ddgi-batch-order reverse",
            "--min-roi-luminance-gain",
            "--min-roi-channel-share-gain 0.065",
            "--max-roi-channel-share 0.05",
            "--expect-version 5",
            "check_ddgi_correctness.sh --dry-run",
            "check_ddgi_runtime_terrain_edits.sh --dry-run",
            "threshold_provenance=docs/ddgi_transport_acceptance.md",
            "direct-sun-framebuffer=PROVEN",
            "convergence_provenance=docs/ddgi_convergence_calibration.md",
            "summarize_ddgi_convergence.py",
            "--consecutive-iterations 2",
            "--hard-max-iteration 8",
            "check_ddgi_sky_normalization_evidence.py",
        ):
            self.assertIn(contract, output)

    def test_dry_run_uses_committed_thresholds_without_calibration_placeholders(
        self,
    ) -> None:
        result = self.run_runner("--dry-run")

        self.assertEqual(result.returncode, 0, result.stderr)
        for contract in (
            "--min-roi-luminance-gain 0.045",
            "--max-roi-luminance-mean 0.00002",
            "--min-roi-luminance-gain 0.00007",
        ):
            self.assertIn(contract, result.stdout)
        self.assertNotIn("CALIBRATE_", result.stdout + result.stderr)
        self.assertNotIn("missing calibrated threshold", result.stdout + result.stderr)
        self.assertEqual(result.stdout.count("--expect-transport-stage converged"), 8)
        self.assertEqual(
            result.stdout.count("--convergence-max-abs-delta 0.0025"), 8
        )
        self.assertEqual(
            result.stdout.count("--convergence-max-rel-delta 0.02"), 8
        )


if __name__ == "__main__":
    unittest.main()
