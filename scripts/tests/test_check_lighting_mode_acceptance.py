from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
RUNNER = REPO_ROOT / "scripts" / "check_lighting_mode_acceptance.sh"


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


if __name__ == "__main__":
    unittest.main()
