from __future__ import annotations

import os
import subprocess
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
RUNNER = SCRIPTS / "check_ddgi_transport_acceptance.sh"


class CheckDdgiTransportAcceptanceTests(unittest.TestCase):
    def run_runner(
        self, *arguments: str, environment: dict[str, str] | None = None
    ) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        for name in (
            "DDGI_DONOR_MAX_S0_RED_ADVANTAGE",
            "DDGI_DONOR_MIN_S1_RED_ADVANTAGE",
            "DDGI_DONOR_MIN_S1_LUMINANCE_MEAN",
            "DDGI_DOGLEG_MAX_S1_LUMINANCE_MEAN",
            "DDGI_DOGLEG_MIN_S2_LUMINANCE_GAIN",
        ):
            env.pop(name, None)
        if environment is not None:
            env.update(environment)
        return subprocess.run(
            [str(RUNNER), *arguments],
            check=False,
            capture_output=True,
            text=True,
            env=env,
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
            "check_ddgi_correctness.sh --dry-run",
            "check_ddgi_runtime_terrain_edits.sh --dry-run",
            "threshold_provenance=docs/ddgi_transport_acceptance.md",
            "direct-sun-framebuffer=UNPROVEN",
        ):
            self.assertIn(contract, output)

    def test_real_run_requires_calibrated_donor_and_dogleg_thresholds_before_build(
        self,
    ) -> None:
        result = self.run_runner()

        self.assertEqual(result.returncode, 2)
        self.assertIn(
            "missing calibrated threshold DDGI_DONOR_MAX_S0_RED_ADVANTAGE",
            result.stderr,
        )
        self.assertNotIn("cargo build", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
