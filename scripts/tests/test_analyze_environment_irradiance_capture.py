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

HEADER_V3 = struct.Struct("<8s10I2Q4IQI2f2I")


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

    def write_capture_v3(
        self,
        path: Path,
        irradiance_pixels: list[tuple[float, float, float, float]],
        world_pixels: list[tuple[float, float, float, float]],
        *,
        geometry_revision: int = 41,
        radiance_revision: int = 17,
        radiance_model_identity: int = 0xA11CE,
        token_serial: int = 9001,
        transport_stage: int = 2,
        transport_iteration: int = 1,
        source_stage: int = 1,
        source_iteration: int = 0,
        source_identity: int = 9001,
        publication_state: int = 1,
        max_abs_delta: float = 0.0125,
        max_rel_delta: float = 0.025,
        nonfinite_count: int = 0,
        valid_count: int | None = None,
    ) -> None:
        self.assertEqual(len(irradiance_pixels), len(world_pixels))
        if valid_count is None:
            valid_count = len(irradiance_pixels)
        header = HEADER_V3.pack(
            analyzer.MAGIC,
            3,
            len(irradiance_pixels),
            1,
            4,
            1,
            16,
            4,
            2,
            geometry_revision,
            radiance_revision,
            radiance_model_identity,
            token_serial,
            transport_stage,
            transport_iteration,
            source_stage,
            source_iteration,
            source_identity,
            publication_state,
            max_abs_delta,
            max_rel_delta,
            nonfinite_count,
            valid_count,
        )
        payload = b"".join(
            analyzer.PIXEL.pack(*pixel)
            for pixel in irradiance_pixels + world_pixels
        )
        path.write_bytes(header + payload)

    def test_loads_v3_metadata_and_two_float4_planes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "capture-v3.rfirr"
            self.write_capture_v3(
                capture_path,
                [(0.1, 0.2, 0.3, 1.0), (0.0, 0.0, 0.0, 0.0)],
                [(1.0, 2.0, 3.0, 1.0), (0.0, 0.0, 0.0, 0.0)],
            )

            capture = analyzer.load_capture(capture_path)

        self.assertEqual(capture.version, 3)
        self.assertEqual(capture.plane_count, 2)
        self.assertEqual(capture.geometry_revision, 41)
        self.assertEqual(capture.radiance_revision, 17)
        self.assertEqual(capture.radiance_model_identity, 0xA11CE)
        self.assertEqual(capture.token_serial, 9001)
        self.assertEqual(capture.transport_stage, 2)
        self.assertEqual(capture.transport_iteration, 1)
        self.assertEqual(capture.source_stage, 1)
        self.assertEqual(capture.source_iteration, 0)
        self.assertEqual(capture.source_identity, 9001)
        self.assertEqual(capture.publication_state, 1)
        self.assertAlmostEqual(capture.max_abs_delta, 0.0125)
        self.assertAlmostEqual(capture.max_rel_delta, 0.025)
        self.assertEqual(capture.nonfinite_count, 0)
        self.assertEqual(capture.valid_count, 2)
        self.assertEqual(
            list(analyzer.PIXEL.iter_unpack(capture.world_payload)),
            [(1.0, 2.0, 3.0, 1.0), (0.0, 0.0, 0.0, 0.0)],
        )

    def test_v3_summary_reports_world_roi_and_channel_metrics(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "world-roi.rfirr"
            self.write_capture_v3(
                capture_path,
                [(0.1, 0.2, 0.3, 1.0), (4.0, 5.0, 6.0, 1.0)],
                [(1.0, 2.0, 3.0, 1.0), (10.0, 2.0, 3.0, 0.0)],
            )

            summary = analyzer.summarize(
                analyzer.load_capture(capture_path),
                world_roi=(0.0, 0.0, 0.0, 5.0, 5.0, 5.0),
            )

        self.assertEqual(summary["world_roi_terrain_hit_count"], 1)
        self.assertEqual(summary["world_position_min"], [1.0, 2.0, 3.0])
        self.assertEqual(summary["world_position_max"], [1.0, 2.0, 3.0])
        for actual, expected in zip(
            summary["rgb_channel_abs_max"], [0.1, 0.2, 0.3]
        ):
            self.assertAlmostEqual(actual, expected)
        self.assertEqual(summary["rgb_channel_nonzero_count"], [1, 1, 1])
        self.assertEqual(summary["exact_direct_sun_visibility_mean"], 1.0)

    def test_v3_comparison_requires_matching_source_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first.rfirr"
            second_path = Path(directory) / "second.rfirr"
            pixels = [(0.1, 0.2, 0.3, 1.0)]
            world = [(1.0, 2.0, 3.0, 1.0)]
            self.write_capture_v3(first_path, pixels, world, source_identity=9001)
            self.write_capture_v3(
                second_path,
                pixels,
                world,
                source_identity=9002,
                radiance_model_identity=0xBEE,
            )

            comparison = analyzer.compare(
                analyzer.load_capture(first_path),
                analyzer.load_capture(second_path),
            )

        self.assertFalse(comparison["compatible"])
        self.assertIn("source_identity", comparison["metadata_mismatches"])
        self.assertIn("radiance_model_identity", comparison["metadata_mismatches"])
        self.assertFalse(comparison["bit_exact"])

    def test_cli_rejects_wrong_transport_stage_and_geometry_revision(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "identity.rfirr"
            self.write_capture_v3(
                capture_path,
                [(0.1, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 1.0)],
            )

            result = self.run_analyzer(
                capture_path,
                "--expect-geometry-revision",
                "40",
                "--expect-transport-stage",
                "seed-sky",
            )

        self.assertEqual(result.returncode, 1, result.stderr)
        failures = json.loads(result.stdout)["validation_failures"]
        self.assertIn("geometry_revision: expected 40, got 41", failures)
        self.assertIn(
            "transport_stage: expected seed-sky, got single-bounce", failures
        )

    def test_cli_rejects_header_nonfinite_count(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "nonfinite.rfirr"
            self.write_capture_v3(
                capture_path,
                [(0.1, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 1.0)],
                nonfinite_count=1,
                valid_count=12,
            )

            result = self.run_analyzer(capture_path)

        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["capture"]["header_nonfinite_count"], 1)
        self.assertIn("header_nonfinite_count: expected 0, got 1", report["validation_failures"])

    def test_cli_rejects_nonfinite_world_plane_even_for_miss(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "nonfinite-world.rfirr"
            self.write_capture_v3(
                capture_path,
                [(0.1, 0.2, 0.3, 1.0), (0.0, 0.0, 0.0, 0.0)],
                [(1.0, 2.0, 3.0, 1.0), (float("nan"), 0.0, 0.0, 0.0)],
            )

            result = self.run_analyzer(capture_path)

        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn(
            "payload contains nonfinite values",
            json.loads(result.stdout)["validation_failures"],
        )

    def test_cli_rejects_converged_capture_above_delta_threshold(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "false-convergence.rfirr"
            self.write_capture_v3(
                capture_path,
                [(0.1, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 1.0)],
                transport_stage=4,
                transport_iteration=3,
                max_abs_delta=0.02,
                max_rel_delta=0.005,
            )

            result = self.run_analyzer(
                capture_path,
                "--convergence-max-abs-delta",
                "0.01",
                "--convergence-max-rel-delta",
                "0.01",
            )

        self.assertEqual(result.returncode, 1, result.stderr)
        failures = json.loads(result.stdout)["validation_failures"]
        self.assertIn("max_abs_delta: converged value 0.02 exceeds 0.01", failures)

    def test_cli_rejects_nonfinite_convergence_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "nonfinite-delta.rfirr"
            self.write_capture_v3(
                capture_path,
                [(0.1, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 1.0)],
                transport_stage=4,
                transport_iteration=6,
                max_abs_delta=float("nan"),
            )

            result = self.run_analyzer(capture_path)

        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn(
            "capture metadata contains nonfinite convergence values",
            json.loads(result.stdout)["validation_failures"],
        )

    def test_v3_sealed_zero_gate_rejects_any_nonzero_rgb_channel(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "sealed.rfirr"
            self.write_capture_v3(
                capture_path,
                [(0.0, 0.0, 0.0000001, 1.0)],
                [(1.0, 2.0, 3.0, 0.0)],
            )

            result = self.run_analyzer(capture_path, "--require-zero-rgb")

        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["capture"]["rgb_channel_nonzero_count"], [0, 0, 1])

    def test_v3_nonnegative_gate_rejects_negative_terrain_hit_channel(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "negative-rgb.rfirr"
            self.write_capture_v3(
                capture_path,
                [(-0.000001, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 0.0)],
            )

            result = self.run_analyzer(capture_path, "--require-nonnegative-rgb")

        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["capture"]["rgb_channel_negative_count"], [1, 0, 0])
        self.assertLess(report["capture"]["rgb_channel_min"][0], 0.0)
        self.assertIn(
            "terrain-hit RGB contains negative channel values",
            report["validation_failures"],
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

    def test_loads_legacy_v1_capture_with_implicit_final_view(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "legacy-v1.rfirr"
            header = analyzer.HEADER_V1.pack(
                analyzer.MAGIC, 1, 1, 1, 4, 1, 32
            )
            capture_path.write_bytes(
                header + analyzer.PIXEL.pack(0.1, 0.2, 0.3, 1.0)
            )

            capture = analyzer.load_capture(capture_path)

        self.assertEqual(capture.version, 1)
        self.assertEqual(capture.debug_view, 0)
        self.assertEqual(capture.plane_count, 1)
        self.assertEqual(capture.world_payload, b"")

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
