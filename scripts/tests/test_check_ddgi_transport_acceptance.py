from __future__ import annotations

import os
import stat
import subprocess
import tempfile
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

    def run_fake_runner(
        self,
        *,
        fail_dogleg: bool = False,
        fail_runtime_child: bool = False,
        fail_summarizer: bool = False,
        fail_tee: bool = False,
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "scripts"
            scripts_lib = scripts / "lib"
            fake_bin = root / "bin"
            scripts.mkdir()
            scripts_lib.mkdir()
            fake_bin.mkdir()

            def executable(path: Path, contents: str) -> None:
                path.write_text(contents, encoding="utf-8")
                path.chmod(path.stat().st_mode | stat.S_IXUSR)

            executable(scripts / RUNNER.name, RUNNER.read_text(encoding="utf-8"))
            executable(
                scripts_lib / "capture_process_evidence.sh",
                (SCRIPTS / "lib" / "capture_process_evidence.sh").read_text(
                    encoding="utf-8"
                ),
            )
            executable(
                scripts / "validate_capture_process_evidence.py",
                (SCRIPTS / "validate_capture_process_evidence.py").read_text(
                    encoding="utf-8"
                ),
            )
            executable(
                fake_bin / "cargo",
                """#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "build" ]]; then exit 0; fi
if [[ "${RUST_LOG:-}" != *"re_flora::ddgi_convergence_evidence=debug"* ]]; then
    echo "missing dedicated convergence evidence target" >&2
    exit 91
fi
if [[ "${RUST_LOG:-}" == *"re_flora::tracer=debug"* ]]; then
    echo "broad tracer debug must remain disabled" >&2
    exit 92
fi
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
[ENV_LIGHT_TEST] first DDGI build verified build_token_serial=1 geometry_revision=2 visible_terrain_publication_revision=2
[ENV_IRRADIANCE_CAPTURE] saved $capture
[ENV_IRRADIANCE_CAPTURE] complete; exiting one-shot capture run"
        printf '%s\n%s\n' "$marker" "$events" >"$run_log"
        printf '%s\n%s\n' "$marker" "$events"
    fi
done
""",
            )
            if fail_tee:
                executable(
                    fake_bin / "tee",
                    '#!/usr/bin/env bash\n/usr/bin/tee "$@"\nexit 9\n',
                )
            executable(
                scripts / "analyze_environment_irradiance_capture.py",
                """#!/usr/bin/env bash
if [[ "${FAKE_FAIL_DOGLEG:-0}" == 1 && "$1" == *dogleg* ]]; then exit 1; fi
printf '{}\n'
""",
            )
            executable(
                scripts / "summarize_ddgi_convergence.py",
                """#!/usr/bin/env bash
if [[ "${FAKE_FAIL_SUMMARIZER:-0}" == 1 ]]; then exit 47; fi
arguments=("$@")
for ((index = 0; index < ${#arguments[@]}; ++index)); do
    if [[ "${arguments[$index]}" == "--output" ]]; then
        : >"${arguments[$((index + 1))]}"
    fi
done
""",
            )
            executable(
                scripts / "check_ddgi_correctness.sh", "#!/usr/bin/env bash\nexit 0\n"
            )
            executable(
                scripts / "check_ddgi_runtime_terrain_edits.sh",
                f"#!/usr/bin/env bash\nexit {1 if fail_runtime_child else 0}\n",
            )
            executable(
                scripts / "check_ddgi_lifecycle_acceptance.sh",
                "#!/usr/bin/env bash\nexit 0\n",
            )
            (scripts / "check_ddgi_sky_normalization_evidence.py").write_text(
                "raise SystemExit(0)\n", encoding="utf-8"
            )
            environment = os.environ.copy()
            environment.update(
                {
                    "PATH": f"{fake_bin}:{environment['PATH']}",
                    "DDGI_TRANSPORT_ACCEPTANCE_OUTPUT_DIR": str(root / "output"),
                    "FAKE_FAIL_DOGLEG": "1" if fail_dogleg else "0",
                    "FAKE_FAIL_SUMMARIZER": "1" if fail_summarizer else "0",
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
            "filter-history-outcome=REQUIRED",
            "--expect-version 8",
            "check_ddgi_correctness.sh --dry-run",
            "check_ddgi_runtime_terrain_edits.sh --dry-run",
            "threshold_provenance=docs/ddgi_transport_acceptance.md",
            "direct-sun-framebuffer=REQUIRED",
            "convergence_provenance=docs/ddgi_convergence_calibration.md",
            "convergence-policy=RUNTIME_LOG source=DDGI_CONVERGENCE_POLICY",
            "summarize_ddgi_convergence.py",
            "check_ddgi_sky_normalization_evidence.py",
        ):
            self.assertIn(contract, output)
        self.assertNotIn("filter-history-action=PROVEN", output)
        self.assertNotIn("filter-history-outcome=ACCEPTED", output)
        self.assertNotIn("direct-sun-framebuffer=PROVEN", output)
        self.assertNotIn("--maximum-update-epoch", output)

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

    def test_dogleg_failure_never_claims_filter_history_outcome(self) -> None:
        result = self.run_fake_runner(fail_dogleg=True)

        self.assertEqual(result.returncode, 1)
        self.assertNotIn(
            "filter-history-outcome=ACCEPTED", result.stdout + result.stderr
        )
        self.assertNotIn("filter-history-action=PROVEN", result.stdout + result.stderr)

    def test_tee_failure_rejects_transport_capture_evidence(self) -> None:
        result = self.run_fake_runner(fail_tee=True)

        self.assertEqual(result.returncode, 1)
        self.assertIn("tee_status=9", result.stderr)
        self.assertNotIn("filter-history-outcome=ACCEPTED", result.stdout)

    def test_transport_requires_the_dual_stream_convergence_summarizer(self) -> None:
        result = self.run_fake_runner(fail_summarizer=True)

        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL convergence provenance summary", result.stderr)

    def test_direct_sun_proof_follows_successful_runtime_child(self) -> None:
        failed = self.run_fake_runner(fail_runtime_child=True)
        succeeded = self.run_fake_runner()

        self.assertEqual(failed.returncode, 1)
        self.assertNotIn("direct-sun-framebuffer=PROVEN", failed.stdout + failed.stderr)
        self.assertEqual(succeeded.returncode, 0, succeeded.stderr)
        self.assertEqual(succeeded.stdout.count("direct-sun-framebuffer=PROVEN"), 1)
        self.assertEqual(succeeded.stdout.count("filter-history-outcome=ACCEPTED"), 1)
        self.assertNotIn("filter-history-action=PROVEN", succeeded.stdout)


if __name__ == "__main__":
    unittest.main()
