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
    def run_fake_wrapper(
        self, screenshot_kind: str
    ) -> tuple[subprocess.CompletedProcess[str], str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fake_bin = root / "bin"
            fake_bin.mkdir()
            source_png = root / "source.png"
            if screenshot_kind == "green":
                write_png(
                    source_png, [[(0, 0, 0) for _ in range(100)] for _ in range(100)]
                )
            elif screenshot_kind == "red":
                write_png(
                    source_png,
                    [[(15, 45, 91) for _ in range(100)] for _ in range(100)],
                )
            elif screenshot_kind == "error":
                source_png.write_bytes(b"not a png")
            else:
                self.fail(f"unknown screenshot kind {screenshot_kind}")
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
            cargo_arguments = args_log.read_text(encoding="utf-8")

        return result, cargo_arguments

    def test_declared_bd2_snapshot_reaches_the_screenshot_analyzer(self) -> None:
        result, cargo_arguments = self.run_fake_wrapper("green")

        self.assertEqual(result.returncode, 0, result.stderr)
        snapshot_file = tomllib.loads(
            (REPO_ROOT / "config" / "camera_snapshots.toml").read_text(encoding="utf-8")
        )
        snapshot = next(
            snapshot
            for snapshot in snapshot_file["snapshots"]
            if snapshot["name"] == "bd2"
        )
        self.assertEqual(snapshot["position"], [0.4920328, 0.5331251, 0.65610564])
        self.assertEqual(snapshot["yaw_deg"], -58.670998)
        self.assertEqual(snapshot["pitch_deg"], -16.57752)
        self.assertEqual(snapshot["fov_deg"], 55.0)
        self.assertIs(snapshot["fly_mode"], False)
        self.assertIn("--screenshot bd2", cargo_arguments)
        self.assertIn("--screenshot bd2", result.stdout)
        self.assertIn("[BD2_BLUE_VOXEL] verdict=GREEN", result.stdout)
        self.assertIn("[BD2_BLUE_VOXEL_CHECK] verdict=GREEN", result.stdout)

    def test_analyzer_red_status_is_preserved_by_the_wrapper(self) -> None:
        result, _ = self.run_fake_wrapper("red")

        self.assertEqual(result.returncode, 1)
        self.assertIn("[BD2_BLUE_VOXEL] verdict=RED", result.stdout)
        self.assertIn("[BD2_BLUE_VOXEL_CHECK] verdict=RED", result.stdout)

    def test_analyzer_error_maps_to_wrapper_status_five(self) -> None:
        result, _ = self.run_fake_wrapper("error")

        self.assertEqual(result.returncode, 5)
        self.assertIn("[BD2_BLUE_VOXEL] verdict=ERROR", result.stdout + result.stderr)
        self.assertIn(
            "[BD2_BLUE_VOXEL_CHECK] verdict=ANALYSIS_FAILED status=2",
            result.stdout + result.stderr,
        )


if __name__ == "__main__":
    unittest.main()
