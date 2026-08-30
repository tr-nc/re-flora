from __future__ import annotations

import os
import stat
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

    def run_fake_success_runner(self, dirty_source: str = "") -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "scripts"
            fake_bin = root / "bin"
            scripts.mkdir()
            fake_bin.mkdir()

            def executable(path: Path, contents: str) -> None:
                path.write_text(contents, encoding="utf-8")
                path.chmod(path.stat().st_mode | stat.S_IXUSR)

            executable(scripts / RUNNER.name, RUNNER.read_text(encoding="utf-8"))
            executable(
                scripts / "validate_capture_process_evidence.py",
                (SCRIPTS / "validate_capture_process_evidence.py").read_text(
                    encoding="utf-8"
                ),
            )
            executable(
                scripts / "analyze_environment_irradiance_capture.py",
                "#!/usr/bin/env bash\nprintf '{}\\n'\n",
            )
            executable(
                fake_bin / "cargo",
                """#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "build" ]]; then exit 0; fi
arguments=("$@")
for ((index = 0; index < ${#arguments[@]}; ++index)); do
    if [[ "${arguments[$index]}" == "--environment-irradiance-capture" ]]; then
        capture="${arguments[$((index + 1))]}"
        mkdir -p "$(dirname "$capture")"
        : >"$capture"
        run_log="${capture%.rfirr}.run.log"
        marker="[RUN_LOG] path=$run_log"
        events="[ENV_LIGHT_TEST] static terrain ready case=sealed terrain_revision=2 settling_frames=2
[DDGI] initialization requested terrain_revision=2 spacing_voxels=32
[ENV_LIGHT_TEST] first DDGI build verified build_token_serial=1 geometry_revision=2 visible_terrain_publication_revision=2"
        run_log_extra=""
        console_extra=""
        if [[ "${FAKE_DIRTY_SOURCE:-}" == "runlog" ]]; then run_log_extra="VUID-dirty"; fi
        if [[ "${FAKE_DIRTY_SOURCE:-}" == "console" ]]; then console_extra="ERROR dirty"; fi
        printf '%s\n%s\n%s\n' "$marker" "$events" "$run_log_extra" >"$run_log"
        printf '%s\n%s\n%s\n' "$marker" "$events" "$console_extra"
    fi
done
""",
            )
            environment = dict(os.environ)
            environment.update(
                {
                    "PATH": f"{fake_bin}:{environment['PATH']}",
                    "DDGI_CORRECTNESS_OUTPUT_DIR": str(root / "captures"),
                    "FAKE_DIRTY_SOURCE": dirty_source,
                }
            )
            return subprocess.run(
                [str(scripts / RUNNER.name)],
                cwd=root,
                env=environment,
                check=False,
                capture_output=True,
                text=True,
                timeout=30,
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
            "unoccluded-irradiance.rfirr --correctness --expect-version 8 "
            "--require-nonnegative-rgb --expect-debug-view unoccluded-irradiance "
            "--reference",
            output,
        )
        self.assertIn(
            "final-a.rfirr --min-reference-error-p99 0.01",
            output,
        )
        self.assertIn(
            "equal-weight-irradiance.rfirr --correctness --expect-version 8 "
            "--require-nonnegative-rgb --expect-debug-view equal-weight-irradiance "
            "--reference",
            output,
        )
        self.assertIn("unoccluded-irradiance.rfirr --min-reference-error-p99 0.01", output)
        self.assertIn("equal-weight-irradiance.rfirr --min-reference-error-p99 0.01", output)

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

    def test_exit_zero_capture_with_dirty_console_or_bound_run_log_fails(self) -> None:
        clean = self.run_fake_success_runner()
        self.assertEqual(clean.returncode, 0, clean.stderr)

        for dirty_source in ("console", "runlog"):
            with self.subTest(dirty_source=dirty_source):
                dirty = self.run_fake_success_runner(dirty_source)
                self.assertEqual(dirty.returncode, 1)
                self.assertIn("process evidence", dirty.stderr)
                self.assertIn("failures=48", dirty.stdout)


if __name__ == "__main__":
    unittest.main()
