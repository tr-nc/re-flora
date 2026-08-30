from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
RUNNER = SCRIPTS / "check_ddgi_correctness.sh"


class CheckDdgiCorrectnessTests(unittest.TestCase):
    def run_runner(
        self, *arguments: str, env: dict[str, str] | None = None
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(RUNNER), *arguments],
            check=False,
            capture_output=True,
            text=True,
            env=env,
        )

    def test_dry_run_captures_and_validates_each_observable_debug_route(self) -> None:
        result = self.run_runner("--dry-run")

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
        self.assertIn("--max-reference-error-max 0.00001", output)
        self.assertIn("--debug-baseline", output)
        self.assertIn("--min-debug-roi-luminance-gain", output)
        self.assertIn("views=8", output)

        capture_commands = [
            line for line in output.splitlines() if line.startswith("cargo run ")
        ]
        self.assertEqual(len(capture_commands), 48)
        for case_name in ("sealed", "portal", "walls"):
            for spacing in (32, 16):
                tuple_commands = [
                    line
                    for line in capture_commands
                    if f"--environment-lighting-test-scene {case_name}" in line
                    and f"--environment-probe-spacing-voxels {spacing}" in line
                ]
                self.assertEqual(len(tuple_commands), 8)
                self.assertEqual(
                    sum("--ddgi-debug-view final " in line for line in tuple_commands),
                    2,
                )

        self.assertIn("--max-reference-error-p99 0.01 --compare", output)
        self.assertIn("walls-spacing32-exact-irradiance.rfirr --max-reference-error-p99 0.40", output)
        self.assertIn("walls-spacing16-exact-irradiance.rfirr --max-reference-error-p99 0.375", output)
        self.assertIn(
            "equal-weight-irradiance.rfirr --correctness --expect-version 9 "
            "--require-nonnegative-rgb --expect-debug-view equal-weight-irradiance "
            "--reference",
            output,
        )
        self.assertIn("unoccluded-irradiance.rfirr --min-reference-error-p99 0.01", output)
        self.assertIn("equal-weight-irradiance.rfirr --min-reference-error-p99 0.01", output)
        self.assertEqual(
            output.count("--min-filter-visibility-reject-count 1"),
            2,
            "only the two production walls final captures prove owner rejection",
        )

    def test_capture_failures_are_accumulated_across_the_complete_matrix(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake_bin = root / "bin"
            fake_bin.mkdir()
            fake_cargo = fake_bin / "cargo"
            fake_cargo.write_text(
                "#!/usr/bin/env bash\n"
                "if [[ \"$1\" == \"build\" ]]; then exit 0; fi\n"
                "echo FAKE_CAPTURE_FAILURE\n"
                "exit 7\n"
            )
            fake_cargo.chmod(0o755)
            env = dict(os.environ)
            env["PATH"] = f"{fake_bin}:{env['PATH']}"
            env["DDGI_CORRECTNESS_OUTPUT_DIR"] = str(root / "captures")

            result = self.run_runner(env=env)

        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(result.stdout.count("FAKE_CAPTURE_FAILURE"), 48)
        self.assertIn("failures=48", result.stdout)
        self.assertEqual(result.stdout.count("backend=ddgi view="), 48)


if __name__ == "__main__":
    unittest.main()
