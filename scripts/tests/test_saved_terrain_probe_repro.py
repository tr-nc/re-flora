from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
RUNNER = SCRIPTS / "repro_saved_terrain_probe_seam.sh"
CHECKER = SCRIPTS / "check_saved_terrain_ddgi_seam.sh"


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

    def test_single_checker_dry_run_is_a_narrow_exact_capture_loop(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            output_dir = Path(directory) / "single"
            result = subprocess.run(
                [str(CHECKER), "--dry-run", str(output_dir)],
                check=False,
                capture_output=True,
                text=True,
                env=os.environ.copy(),
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(output_dir.exists())
        self.assertIn("--terrain-load saves/terrain_snapshot.rflterrain", result.stdout)
        self.assertIn("--ddgi-debug-view exact-irradiance", result.stdout)
        self.assertIn("--environment-probe-visualization", result.stdout)
        self.assertIn("--screenshot-delay 10", result.stdout)
        self.assertIn("--auto-exit 18", result.stdout)
        self.assertIn("magick", result.stdout)
        self.assertIn("analyze_saved_ddgi_seam.py", result.stdout)

    def test_single_checker_is_executable(self) -> None:
        self.assertTrue(os.access(CHECKER, os.X_OK))

    def test_single_checker_reports_missing_fixture_before_launching_app(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            missing_terrain = Path(directory) / "missing.rflterrain"
            result = subprocess.run(
                [str(CHECKER), str(Path(directory) / "single")],
                check=False,
                capture_output=True,
                text=True,
                env={**os.environ, "TERRAIN_SNAPSHOT_PATH": str(missing_terrain)},
            )

        self.assertEqual(result.returncode, 2)
        self.assertIn("verdict=INPUT_MISSING", result.stderr)


if __name__ == "__main__":
    unittest.main()
