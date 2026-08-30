from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
RUNNER = REPO_ROOT / "scripts" / "check_lighting_mode_acceptance.sh"


def executable(path: Path, source: str) -> Path:
    path.write_text(source)
    path.chmod(0o755)
    return path


class LightingModeAcceptanceRunnerTests(unittest.TestCase):
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
        self.assertIn("analyzer-command=", result.stdout)
        self.assertFalse(artifact.exists())
        self.assertFalse(Path(f"{artifact}.app.log").exists())

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
            analyzer = executable(
                root / "analyzer",
                "#!/usr/bin/env bash\n"
                "printf '%s\\n' '{\"verdict\": \"GREEN\"}'\n",
            )

            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_ANALYZER": str(analyzer),
                    "REFLORA_LOG_POINTER": str(log_pointer),
                    "BOUND_RUN_LOG": str(bound_log),
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
            analyzer = executable(
                root / "analyzer",
                "#!/usr/bin/env bash\n"
                "printf '%s\\n' '[LIGHTING_MODE_ACCEPTANCE] verdict=RED reason=raw-alpha-drift' >&2\n"
                "exit 1\n",
            )

            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_ANALYZER": str(analyzer),
                    "BOUND_RUN_LOG": str(bound_log),
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
