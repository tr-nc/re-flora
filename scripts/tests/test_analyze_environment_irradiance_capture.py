from __future__ import annotations

import json
import struct
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import analyze_environment_irradiance_capture as analyzer  # noqa: E402


class AnalyzeEnvironmentIrradianceCaptureTests(unittest.TestCase):
    def write_capture(
        self, path: Path, pixels: list[tuple[float, float, float, float]]
    ) -> None:
        header = analyzer.HEADER_V2.pack(
            analyzer.MAGIC, 2, len(pixels), 1, 4, 1, 32, 0
        )
        payload = b"".join(analyzer.PIXEL.pack(*pixel) for pixel in pixels)
        path.write_bytes(header + payload)

    def run_analyzer(
        self, capture: Path, *arguments: str
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(SCRIPTS / "analyze_environment_irradiance_capture.py"),
                str(capture),
                *arguments,
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_zero_rgb_summary_ignores_negative_zero_and_non_hit_rgb(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "zero.rfirr"
            self.write_capture(
                capture_path,
                [(0.0, -0.0, 0.0, 1.0), (1.0, 2.0, 3.0, 0.0)],
            )

            summary = analyzer.summarize(analyzer.load_capture(capture_path))

        self.assertEqual(summary["terrain_hit_count"], 1)
        self.assertEqual(summary["rgb_abs_max"], 0.0)
        self.assertEqual(summary["rgb_nonzero_count"], 0)

    def test_require_zero_rgb_rejects_value_that_passes_luminance_gate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "tiny-nonzero.rfirr"
            self.write_capture(capture_path, [(0.0000001, 0.0, 0.0, 1.0)])

            luminance_only = self.run_analyzer(
                capture_path, "--max-luminance", "0.00001"
            )
            exact_zero = self.run_analyzer(
                capture_path,
                "--max-luminance",
                "0.00001",
                "--require-zero-rgb",
            )

        self.assertEqual(luminance_only.returncode, 0, luminance_only.stderr)
        self.assertEqual(exact_zero.returncode, 1, exact_zero.stderr)
        report = json.loads(exact_zero.stdout)
        self.assertGreater(report["capture"]["rgb_abs_max"], 0.0)
        self.assertEqual(report["capture"]["rgb_nonzero_count"], 1)


if __name__ == "__main__":
    unittest.main()
