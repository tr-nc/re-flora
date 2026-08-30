from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
RUNNER = REPO_ROOT / "scripts" / "check_lighting_mode_acceptance.sh"


def executable(path: Path, source: str) -> Path:
    path.write_text(source)
    path.chmod(0o755)
    return path


def python_analyzer(path: Path, output: str, status: int = 0) -> Path:
    return executable(
        path,
        "#!/usr/bin/env python3\n"
        "import sys\n"
        f"print({output!r}, file={'sys.stderr' if status else 'sys.stdout'})\n"
        f"raise SystemExit({status})\n",
    )


def test_analyzer_environment(analyzer: Path) -> dict[str, str]:
    return {
        "REFLORA_LIGHTING_MODE_ACCEPTANCE_TEST_ONLY": "1",
        "REFLORA_LIGHTING_MODE_ACCEPTANCE_TEST_ANALYZER": str(analyzer),
    }


class LightingModeAcceptanceRunnerTests(unittest.TestCase):
    def test_production_ignores_arbitrary_analyzer_override(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            bound_log = root / "bound.run.log"
            bound_log.write_text("clean\n")
            override_ran = root / "override-ran"
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "artifact=\"${@: -1}\"\n"
                "printf not-a-production-artifact > \"$artifact\"\n"
                "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n",
            )
            malicious = executable(
                root / "analyzer",
                "#!/usr/bin/env bash\n"
                "printf ran > \"$OVERRIDE_RAN\"\n"
                "printf '%s\\n' '{\"schema\": \"re-flora-lighting-mode-acceptance-v1\", \"calibration\": \"r13-e2-production-v1\", \"verdict\": \"GREEN\"}'\n",
            )
            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_ANALYZER": str(malicious),
                    "OVERRIDE_RAN": str(override_ran),
                    "BOUND_RUN_LOG": str(bound_log),
                },
                capture_output=True,
                text=True,
                check=False,
            )
            override_executed = override_ran.exists()

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("verdict=ANALYZER_FAILED", result.stderr)
        self.assertFalse(override_executed)
        self.assertNotIn("verdict=GREEN", result.stdout)

    def test_explicit_test_analyzer_can_only_emit_test_green(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            bound_log = root / "bound.run.log"
            bound_log.write_text("clean\n")
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "artifact=\"${@: -1}\"\n"
                "printf fixture > \"$artifact\"\n"
                "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n",
            )
            analyzer = python_analyzer(
                root / "analyzer.py",
                '{"schema":"re-flora-lighting-mode-acceptance-v1","calibration":"r13-e2-production-v1","verdict":"GREEN"}',
            )
            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "BOUND_RUN_LOG": str(bound_log),
                    **test_analyzer_environment(analyzer),
                },
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("verdict=TEST_GREEN", result.stdout)
        self.assertNotIn("verdict=GREEN", result.stdout)

    def test_test_analyzer_without_explicit_test_mode_is_rejected_without_state(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            analyzer = python_analyzer(root / "analyzer.py", "{}")
            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_LIGHTING_MODE_ACCEPTANCE_TEST_ANALYZER": str(analyzer),
                },
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("reason=test-analyzer-requires-test-only-mode", result.stderr)
        self.assertFalse(artifact.exists())
        self.assertFalse(Path(f"{artifact}.app.log").exists())

    def test_analyzer_json_contract_rejects_wrong_authority_fields(self) -> None:
        cases = (
            "not-json",
            '{"schema":"wrong","calibration":"r13-e2-production-v1","verdict":"GREEN"}',
            '{"schema":"re-flora-lighting-mode-acceptance-v1","calibration":"wrong","verdict":"GREEN"}',
            '{"schema":"re-flora-lighting-mode-acceptance-v1","calibration":"r13-e2-production-v1","verdict":"RED"}',
        )
        for output in cases:
            with self.subTest(output=output), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                artifact = root / "capture.rflma"
                bound_log = root / "bound.run.log"
                bound_log.write_text("clean\n")
                cargo = executable(
                    root / "cargo",
                    "#!/usr/bin/env bash\n"
                    "artifact=\"${@: -1}\"\n"
                    "printf fixture > \"$artifact\"\n"
                    "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n",
                )
                analyzer = python_analyzer(root / "analyzer.py", output)
                result = subprocess.run(
                    [str(RUNNER), str(artifact)],
                    cwd=REPO_ROOT,
                    env={
                        **os.environ,
                        "REFLORA_CARGO": str(cargo),
                        "BOUND_RUN_LOG": str(bound_log),
                        **test_analyzer_environment(analyzer),
                    },
                    capture_output=True,
                    text=True,
                    check=False,
                )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("reason=invalid-analyzer-json", result.stderr)
            self.assertNotIn("verdict=TEST_GREEN", result.stdout)
            self.assertNotIn("verdict=GREEN", result.stdout)

    def test_preflighted_python_is_the_analyzer_interpreter(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            bound_log = root / "bound.run.log"
            bound_log.write_text("clean\n")
            python_used = root / "python-used"
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "artifact=\"${@: -1}\"\n"
                "printf fixture > \"$artifact\"\n"
                "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n",
            )
            analyzer = python_analyzer(
                root / "analyzer.py",
                '{"schema":"re-flora-lighting-mode-acceptance-v1","calibration":"r13-e2-production-v1","verdict":"GREEN"}',
            )
            python = executable(
                root / "python",
                "#!/usr/bin/env bash\n"
                "printf used >> \"$PYTHON_USED\"\n"
                f"exec '{sys.executable}' \"$@\"\n",
            )
            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_PYTHON": str(python),
                    "PYTHON_USED": str(python_used),
                    "BOUND_RUN_LOG": str(bound_log),
                    **test_analyzer_environment(analyzer),
                },
                capture_output=True,
                text=True,
                check=False,
            )
            interpreter_invocations = python_used.read_text()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("verdict=TEST_GREEN", result.stdout)
        self.assertGreaterEqual(interpreter_invocations.count("used"), 2)

    def test_exit_zero_with_artifact_rejects_case_insensitive_fatal_log_markers(self) -> None:
        markers = (
            "eRrOr: mixed-case failure",
            "PaNiC in worker",
            "vUiD-vkCmdDispatch-None-00000",
            "Validation failure",
            "DEVICE LOST while reading back",
            "Stale ReadBack observed",
            "VkResult=VK_ERROR_DEVICE_LOST",
            "result=ERROR_DEVICE_LOST",
            "VkResult=VK_ERROR_VALIDATION_FAILED_EXT",
            "VkResult=VK_ERROR_OUT_OF_DEVICE_MEMORY",
            "result=ERROR_FORMAT_NOT_SUPPORTED",
        )
        for marker in markers:
            with self.subTest(marker=marker), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                artifact = root / "capture.rflma"
                bound_log = root / "bound.run.log"
                bound_log.write_text(marker + "\n")
                cargo = executable(
                    root / "cargo",
                    "#!/usr/bin/env bash\n"
                    "artifact=\"${@: -1}\"\n"
                    "printf artifact > \"$artifact\"\n"
                    "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n",
                )
                analyzer = python_analyzer(
                    root / "analyzer.py",
                    '{"schema":"re-flora-lighting-mode-acceptance-v1","calibration":"r13-e2-production-v1","verdict":"GREEN"}',
                )
                result = subprocess.run(
                    [str(RUNNER), str(artifact)],
                    cwd=REPO_ROOT,
                    env={
                        **os.environ,
                        "REFLORA_CARGO": str(cargo),
                        "BOUND_RUN_LOG": str(bound_log),
                        **test_analyzer_environment(analyzer),
                    },
                    capture_output=True,
                    text=True,
                    check=False,
                )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("reason=error-marker", result.stderr)
            self.assertIn(marker, result.stderr)

    def test_fatal_log_rg_io_failure_is_not_treated_as_no_match(self) -> None:
        real_rg = shutil.which("rg")
        self.assertIsNotNone(real_rg)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            bound_log = root / "bound.run.log"
            bound_log.write_text("clean\n")
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "artifact=\"${@: -1}\"\n"
                "printf artifact > \"$artifact\"\n"
                "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n",
            )
            analyzer = python_analyzer(
                root / "analyzer.py",
                '{"schema":"re-flora-lighting-mode-acceptance-v1","calibration":"r13-e2-production-v1","verdict":"GREEN"}',
            )
            failing_rg = executable(
                root / "rg",
                "#!/usr/bin/env bash\n"
                "if [[ \" $* \" == *' -i '* ]]; then\n"
                "  printf 'simulated rg I/O failure\\n' >&2\n"
                "  exit 2\n"
                "fi\n"
                f"exec '{real_rg}' \"$@\"\n",
            )
            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_RG": str(failing_rg),
                    "BOUND_RUN_LOG": str(bound_log),
                    **test_analyzer_environment(analyzer),
                },
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("reason=fatal-log-scan-failed", result.stderr)
        self.assertIn("simulated rg I/O failure", result.stderr)

    def test_nonfatal_neighbours_do_not_trip_the_canonical_log_scan(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            bound_log = root / "bound.run.log"
            bound_log.write_text(
                "errors=0\n"
                "validation layers enabled\n"
                "device loss recovery armed\n"
                "stale reads=0\n"
                "panicky metric=false\n"
            )
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "artifact=\"${@: -1}\"\n"
                "printf artifact > \"$artifact\"\n"
                "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n",
            )
            analyzer = python_analyzer(
                root / "analyzer.py",
                '{"schema":"re-flora-lighting-mode-acceptance-v1","calibration":"r13-e2-production-v1","verdict":"GREEN"}',
            )
            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "BOUND_RUN_LOG": str(bound_log),
                    **test_analyzer_environment(analyzer),
                },
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_dry_run_declares_one_release_hidden_app_transaction(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / "capture.rflma"
            result = subprocess.run(
                [str(RUNNER), "--dry-run", str(artifact)],
                cwd=REPO_ROOT,
                env={**os.environ, "REFLORA_CARGO": "/tmp/local-petal-cargo"},
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.count("cargo-command="), 1)
        self.assertIn("/tmp/local-petal-cargo run --release --", result.stdout)
        self.assertIn("--hidden --mute --lighting-mode-acceptance", result.stdout)
        self.assertIn("analyzer-command= python3", result.stdout)
        self.assertIn("scripts/analyze_lighting_mode_acceptance.py", result.stdout)
        self.assertFalse(artifact.exists())
        self.assertFalse(Path(f"{artifact}.app.log").exists())

    def test_missing_execution_dependency_fails_before_writing_state(self) -> None:
        cases = (
            ("REFLORA_CARGO", "definitely-missing-cargo"),
            ("REFLORA_RG", "definitely-missing-rg"),
            ("REFLORA_PYTHON", "definitely-missing-python"),
            ("REFLORA_TIMEOUT", "definitely-missing-timeout"),
            (
                "REFLORA_LIGHTING_MODE_ACCEPTANCE_TEST_ANALYZER",
                "definitely-missing-analyzer",
            ),
        )
        for variable, missing in cases:
            with self.subTest(variable=variable), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                artifact = root / "capture.rflma"
                cargo = executable(root / "cargo", "#!/usr/bin/env bash\nexit 99\n")
                analyzer = python_analyzer(root / "analyzer.py", "unused", status=99)
                env = {
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    **test_analyzer_environment(analyzer),
                    variable: missing,
                }
                result = subprocess.run(
                    [str(RUNNER), str(artifact)],
                    cwd=REPO_ROOT,
                    env=env,
                    capture_output=True,
                    text=True,
                    check=False,
                )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("reason=missing-dependency", result.stderr)
            self.assertIn(missing, result.stderr)
            self.assertFalse(artifact.exists())
            self.assertFalse(Path(f"{artifact}.app.log").exists())

    def test_invalid_timeout_fails_before_writing_state(self) -> None:
        for timeout in ("0", "1201", "not-a-number"):
            with self.subTest(timeout=timeout), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                artifact = root / "capture.rflma"
                cargo = executable(root / "cargo", "#!/usr/bin/env bash\nexit 99\n")
                result = subprocess.run(
                    [str(RUNNER), str(artifact)],
                    cwd=REPO_ROOT,
                    env={
                        **os.environ,
                        "REFLORA_CARGO": str(cargo),
                        "REFLORA_LIGHTING_MODE_ACCEPTANCE_TIMEOUT_SECONDS": timeout,
                    },
                    capture_output=True,
                    text=True,
                    check=False,
                )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("reason=invalid-timeout", result.stderr)
            self.assertFalse(artifact.exists())
            self.assertFalse(Path(f"{artifact}.app.log").exists())

    def test_timeout_fails_explicitly_after_recovering_bound_log(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            bound_log = root / "bound.run.log"
            bound_log.write_text("clean bound log before timeout\n")
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n"
                "sleep 5\n",
            )
            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_LIGHTING_MODE_ACCEPTANCE_TIMEOUT_SECONDS": "1",
                    "BOUND_RUN_LOG": str(bound_log),
                },
                capture_output=True,
                text=True,
                check=False,
                timeout=10,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("verdict=APP_FAILED", result.stderr)
        self.assertIn("reason=timeout", result.stderr)
        self.assertIn(f"path={bound_log}", result.stderr)

    def test_timeout_preserves_runtime_red_from_the_bound_log(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            bound_log = root / "bound.run.log"
            runtime_red = "[LIGHTING_MODE_ACCEPTANCE] verdict=RED reason=readback-timeout-root-cause"
            bound_log.write_text(runtime_red + "\n" + "noise\n" * 120)
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n"
                "sleep 5\n",
            )
            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_LIGHTING_MODE_ACCEPTANCE_TIMEOUT_SECONDS": "1",
                    "BOUND_RUN_LOG": str(bound_log),
                },
                capture_output=True,
                text=True,
                check=False,
                timeout=10,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn(runtime_red, result.stderr)
        self.assertIn("verdict=APP_FAILED reason=timeout", result.stderr)

    def test_timeout_before_run_log_marker_is_classified_as_timeout(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            cargo = executable(root / "cargo", "#!/usr/bin/env bash\nsleep 5\n")
            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_LIGHTING_MODE_ACCEPTANCE_TIMEOUT_SECONDS": "1",
                },
                capture_output=True,
                text=True,
                check=False,
                timeout=10,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("verdict=APP_FAILED reason=timeout", result.stderr)
        self.assertIn("run-log-marker-count=0", result.stderr)

    def test_uses_process_bound_marker_instead_of_concurrent_latest_pointer(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            bound_log = root / "bound.run.log"
            bound_log.write_text("clean bound log\n")
            foreign_log = root / "foreign.run.log"
            foreign_log.write_text(
                "[LIGHTING_MODE_ACCEPTANCE] verdict=RED reason=foreign-process\n"
            )
            log_pointer = root / "latest-run-log.txt"
            log_pointer.write_text(f"{foreign_log}\n")
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "artifact=\"${@: -1}\"\n"
                "printf artifact > \"$artifact\"\n"
                "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n",
            )
            analyzer = python_analyzer(
                root / "analyzer.py",
                '{"schema":"re-flora-lighting-mode-acceptance-v1","calibration":"r13-e2-production-v1","verdict":"GREEN"}',
            )

            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_LOG_POINTER": str(log_pointer),
                    "BOUND_RUN_LOG": str(bound_log),
                    **test_analyzer_environment(analyzer),
                },
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f"log={bound_log}", result.stdout)
        self.assertNotIn("foreign-process", result.stderr)

    def test_reports_analyzer_red_instead_of_overwriting_it_as_missing_log(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            bound_log = root / "bound.run.log"
            bound_log.write_text("clean bound log\n")
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "artifact=\"${@: -1}\"\n"
                "printf artifact > \"$artifact\"\n"
                "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n",
            )
            analyzer = python_analyzer(
                root / "analyzer.py",
                "[LIGHTING_MODE_ACCEPTANCE] verdict=RED reason=raw-alpha-drift",
                status=1,
            )

            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "BOUND_RUN_LOG": str(bound_log),
                    **test_analyzer_environment(analyzer),
                },
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("verdict=RED reason=raw-alpha-drift", result.stderr)
        self.assertNotIn("missing-artifact-or-run-log", result.stderr)

    def test_reports_app_red_before_checking_for_the_fail_closed_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            bound_log = root / "bound.run.log"
            bound_log.write_text(
                "[LIGHTING_MODE_ACCEPTANCE] verdict=RED reason=identity-drift-at-ddgi-field\n"
            )
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n",
            )

            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "BOUND_RUN_LOG": str(bound_log),
                },
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "[LIGHTING_MODE_ACCEPTANCE] verdict=RED reason=identity-drift-at-ddgi-field",
            result.stderr,
        )
        self.assertNotIn("missing-artifact", result.stderr)

    def test_missing_duplicate_or_nonexistent_run_log_marker_fails_closed(self) -> None:
        cases = ("missing", "duplicate", "nonexistent")
        for case in cases:
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                artifact = root / "capture.rflma"
                first_log = root / "first.run.log"
                first_log.write_text("clean\n")
                second_log = root / "second.run.log"
                second_log.write_text("clean\n")
                marker_lines = {
                    "missing": "",
                    "duplicate": (
                        f"printf '[RUN_LOG] path=%s\\n' '{first_log}'\n"
                        f"printf '[RUN_LOG] path=%s\\n' '{second_log}'\n"
                    ),
                    "nonexistent": (
                        f"printf '[RUN_LOG] path=%s\\n' '{root / 'absent.run.log'}'\n"
                    ),
                }[case]
                cargo = executable(
                    root / "cargo",
                    "#!/usr/bin/env bash\n"
                    "artifact=\"${@: -1}\"\n"
                    "printf artifact > \"$artifact\"\n"
                    f"{marker_lines}",
                )

                result = subprocess.run(
                    [str(RUNNER), str(artifact)],
                    cwd=REPO_ROOT,
                    env={**os.environ, "REFLORA_CARGO": str(cargo)},
                    capture_output=True,
                    text=True,
                    check=False,
                )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("run-log-marker", result.stderr)

    def test_nonzero_app_status_preserves_early_bound_runtime_red(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            bound_log = root / "bound.run.log"
            bound_log.write_text(
                "[LIGHTING_MODE_ACCEPTANCE] verdict=RED reason=early-bound-red\n"
                + "".join(f"bound noise {index}\n" for index in range(120))
            )
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n"
                "for index in $(seq 1 120); do printf 'app noise %s\\n' \"$index\" >&2; done\n"
                "exit 7\n",
            )

            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "BOUND_RUN_LOG": str(bound_log),
                },
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "[LIGHTING_MODE_ACCEPTANCE] verdict=RED reason=early-bound-red",
            result.stderr,
        )
        self.assertIn("verdict=APP_REJECTED", result.stderr)
        self.assertNotIn("missing-artifact", result.stderr)

    def test_nonzero_app_status_without_runtime_red_keeps_tail_diagnostic(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            bound_log = root / "bound.run.log"
            bound_log.write_text("clean bound log\n")
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "printf '[RUN_LOG] path=%s\\n' \"$BOUND_RUN_LOG\"\n"
                "printf 'nonzero-tail-diagnostic\\n' >&2\n"
                "exit 9\n",
            )

            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "BOUND_RUN_LOG": str(bound_log),
                },
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("app-status=9", result.stderr)
        self.assertIn("nonzero-tail-diagnostic", result.stderr)


if __name__ == "__main__":
    unittest.main()
