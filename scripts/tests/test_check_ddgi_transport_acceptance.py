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
            for stage in ("e0", "e1", "converged"):
                self.assertIn(
                    f"case=sealed spacing={spacing} target={stage} order=forward",
                    output,
                )
            for stage in ("e0", "converged"):
                self.assertIn(
                    f"case=donor spacing={spacing} target={stage} order=forward",
                    output,
                )
            for stage in ("e0", "e1", "converged"):
                self.assertIn(
                    f"case=dogleg spacing={spacing} target={stage} order=forward",
                    output,
                )
            self.assertIn(
                f"case=portal spacing={spacing} target=converged order=forward",
                output,
            )
            self.assertIn(
                f"case=donor spacing={spacing} target=e0 order=reverse", output
            )

        for contract in (
            "--require-zero-rgb",
            "--expect-lifecycle-state converging",
            "--expect-update-epoch 0",
            "--expect-lifecycle-state converged",
            "--expect-batch-order reverse",
            "--ddgi-batch-order reverse",
            "--min-roi-luminance-gain",
            "--expect-debug-view final",
            "filter-history-action=REQUIRED seam=owner-generated-filter-epoch-v10",
            "analyze_current_environment_irradiance_capture.py",
            "--require-filter-history-retain-blend",
            "--require-filter-local-recovery-policy",
            "--min-filter-visibility-reject-count 1",
            "check_ddgi_correctness.sh --dry-run",
            "check_ddgi_runtime_terrain_edits.sh --dry-run",
            "threshold_provenance=docs/ddgi_transport_acceptance.md",
            "direct-sun-framebuffer=PROVEN",
            "convergence_provenance=docs/ddgi_convergence_calibration.md",
            "summarize_ddgi_convergence.py",
            "--consecutive-epochs 2",
            "--minimum-epoch-count 8",
            "--maximum-update-epoch 63",
            "check_ddgi_sky_normalization_evidence.py",
        ):
            self.assertIn(contract, output)
        self.assertNotIn("filter-history-action=PROVEN", output)
        self.assertNotIn(
            "filter-history-action=PROVEN seam=dogleg-e0-e1-production-capture",
            RUNNER.read_text(),
        )

    def test_dry_run_uses_committed_thresholds_without_calibration_placeholders(
        self,
    ) -> None:
        result = self.run_runner("--dry-run")

        self.assertEqual(result.returncode, 0, result.stderr)
        for contract in (
            "--min-roi-luminance-mean 0.045",
            "--max-roi-luminance-mean 0.00002",
            "--min-roi-luminance-gain 0.000035",
        ):
            self.assertIn(contract, result.stdout)
        self.assertNotIn("CALIBRATE_", result.stdout + result.stderr)
        self.assertNotIn("missing calibrated threshold", result.stdout + result.stderr)
        self.assertEqual(result.stdout.count("--expect-lifecycle-state converged"), 8)

    def test_failed_invocation_never_claims_filter_history_proof(self) -> None:
        result = self.run_runner("--invalid")

        self.assertEqual(result.returncode, 2)
        self.assertNotIn(
            "filter-history-action=PROVEN", result.stdout + result.stderr
        )


if __name__ == "__main__":
    unittest.main()
