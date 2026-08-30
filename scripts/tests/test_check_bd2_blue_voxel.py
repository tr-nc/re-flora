from __future__ import annotations

import os
import stat
import subprocess
import tempfile
import tomllib
import unittest
from pathlib import Path

from scripts.tests.test_analyze_bd2_blue_voxel import write_png


REPO_ROOT = Path(__file__).resolve().parents[2]
RUNNER = REPO_ROOT / "scripts" / "check_bd2_blue_voxel.sh"


class CheckBd2BlueVoxelTests(unittest.TestCase):
    def test_declared_bd2_snapshot_reaches_the_screenshot_analyzer(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake_bin = root / "bin"
            fake_bin.mkdir()
            source_png = root / "source.png"
            write_png(source_png, [[(0, 0, 0) for _ in range(100)] for _ in range(100)])
            run_log = root / "run.log"
            run_log.write_text("application completed\n", encoding="utf-8")
            args_log = root / "cargo.args"
            cargo = fake_bin / "cargo"
            cargo.write_text(
                """#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >"$BD2_FAKE_ARGS_LOG"
arguments=("$@")
for ((index = 0; index < ${#arguments[@]}; ++index)); do
    if [[ "${arguments[$index]}" == "--screenshot" ]]; then
        cp "$BD2_FAKE_SCREENSHOT" "${arguments[$((index + 2))]}"
        break
    fi
done
printf 'Run log saved to %s\n' "$BD2_FAKE_RUN_LOG"
""",
                encoding="utf-8",
            )
            cargo.chmod(cargo.stat().st_mode | stat.S_IXUSR)
            environment = os.environ.copy()
            environment.update(
                {
                    "PATH": f"{fake_bin}:{environment['PATH']}",
                    "BD2_FAKE_ARGS_LOG": str(args_log),
                    "BD2_FAKE_SCREENSHOT": str(source_png),
                    "BD2_FAKE_RUN_LOG": str(run_log),
                }
            )

            result = subprocess.run(
                [str(RUNNER), str(root / "output")],
                cwd=REPO_ROOT,
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        snapshot_file = tomllib.loads(
            (REPO_ROOT / "config" / "camera_snapshots.toml").read_text(encoding="utf-8")
        )
        self.assertIn("bd2", {snapshot["name"] for snapshot in snapshot_file["snapshots"]})
        self.assertIn("--screenshot bd2", result.stdout)
        self.assertIn("[BD2_BLUE_VOXEL] verdict=GREEN", result.stdout)
        self.assertIn("[BD2_BLUE_VOXEL_CHECK] verdict=GREEN", result.stdout)


if __name__ == "__main__":
    unittest.main()
