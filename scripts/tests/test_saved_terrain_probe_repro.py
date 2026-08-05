from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
RUNNER = SCRIPTS / "repro_saved_terrain_probe_seam.sh"


class SavedTerrainProbeReproTests(unittest.TestCase):
    def test_dry_run_uses_saved_terrain_only_and_emits_probe_captures(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output_dir = Path(directory) / "captures"
            result = subprocess.run(
                [str(RUNNER), "--dry-run", str(output_dir)],
                check=False,
                capture_output=True,
                text=True,
                env=os.environ.copy(),
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(output_dir.exists())
        self.assertEqual(result.stdout.count("cargo run --release"), 3)
        self.assertIn("--terrain-load saves/terrain_snapshot.rflterrain", result.stdout)
        self.assertIn("--no-god-rays", result.stdout)
        self.assertIn("--screenshot snapshot", result.stdout)
        self.assertIn("--ddgi-debug-view exact-irradiance", result.stdout)
        self.assertIn("--ddgi-debug-view dominant-probe", result.stdout)
        self.assertIn("--environment-probe-visualization", result.stdout)
        self.assertNotIn("--environment-lighting-test-scene", result.stdout)
        self.assertEqual(result.stdout.count("magick"), 3)

    def test_runner_is_executable(self) -> None:
        self.assertTrue(os.access(RUNNER, os.X_OK))


if __name__ == "__main__":
    unittest.main()
