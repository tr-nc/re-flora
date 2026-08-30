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

    def test_reports_analyzer_red_instead_of_overwriting_it_as_missing_log(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            log_pointer = root / "latest-run-log.txt"
            cargo = root / "cargo"
            cargo.write_text(
                "#!/usr/bin/env bash\n"
                "artifact=\"${@: -1}\"\n"
                "printf artifact > \"$artifact\"\n"
                "run_log=\"$artifact.run.log\"\n"
                "printf clean-run-log > \"$run_log\"\n"
                "printf '%s\\n' \"$run_log\" > \"$REFLORA_LOG_POINTER\"\n"
            )
            cargo.chmod(0o755)
            analyzer = root / "analyzer"
            analyzer.write_text(
                "#!/usr/bin/env bash\n"
                "printf '%s\\n' '[LIGHTING_MODE_ACCEPTANCE] verdict=RED reason=raw-alpha-drift' >&2\n"
                "exit 1\n"
            )
            analyzer.chmod(0o755)

            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_ANALYZER": str(analyzer),
                    "REFLORA_LOG_POINTER": str(log_pointer),
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
            log_pointer = root / "latest-run-log.txt"
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "artifact=\"${@: -1}\"\n"
                "run_log=\"$artifact.run.log\"\n"
                "printf '%s\\n' '[LIGHTING_MODE_ACCEPTANCE] verdict=RED reason=identity-drift-at-ddgi-field' > \"$run_log\"\n"
                "printf '%s\\n' \"$run_log\" > \"$REFLORA_LOG_POINTER\"\n",
            )

            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_LOG_POINTER": str(log_pointer),
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
        self.assertNotIn("missing-artifact-or-run-log", result.stderr)

    def test_stale_latest_pointer_fails_closed_even_when_artifact_exists(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            stale_log = root / "stale.run.log"
            stale_log.write_text("stale")
            log_pointer = root / "latest-run-log.txt"
            log_pointer.write_text(f"{stale_log}\n")
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "artifact=\"${@: -1}\"\n"
                "printf artifact > \"$artifact\"\n",
            )

            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_LOG_POINTER": str(log_pointer),
                },
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing-artifact-or-run-log", result.stderr)

    def test_missing_latest_pointer_fails_closed_even_when_artifact_exists(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifact = root / "capture.rflma"
            log_pointer = root / "missing-latest-run-log.txt"
            cargo = executable(
                root / "cargo",
                "#!/usr/bin/env bash\n"
                "artifact=\"${@: -1}\"\n"
                "printf artifact > \"$artifact\"\n",
            )

            result = subprocess.run(
                [str(RUNNER), str(artifact)],
                cwd=REPO_ROOT,
                env={
                    **os.environ,
                    "REFLORA_CARGO": str(cargo),
                    "REFLORA_LOG_POINTER": str(log_pointer),
                },
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing-artifact-or-run-log", result.stderr)


if __name__ == "__main__":
    unittest.main()
