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
HEADER_V4 = struct.Struct("<8s10I3Q4IQ3I2f2I")
HEADER_V5 = HEADER_V4


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

    def write_capture_v4(
        self,
        path: Path,
        irradiance_pixels: list[tuple[float, float, float, float]],
        world_pixels: list[tuple[float, float, float, float]],
        *,
        build_token_serial: int = 9001,
        field_serial: int = 89,
        source_field_serial: int = 88,
        source_radiance_revision: int = 16,
        batch_order: int = 0,
    ) -> None:
        self.assertEqual(len(irradiance_pixels), len(world_pixels))
        header = HEADER_V4.pack(
            analyzer.MAGIC,
            4,
            len(irradiance_pixels),
            1,
            4,
            1,
            16,
            0,
            2,
            41,
            17,
            0xA11CE,
            build_token_serial,
            field_serial,
            3,
            6,
            3,
            5,
            source_field_serial,
            source_radiance_revision,
            1,
            batch_order,
            0.0125,
            0.025,
            0,
            len(irradiance_pixels),
        )
        payload = b"".join(
            analyzer.PIXEL.pack(*pixel)
            for pixel in irradiance_pixels + world_pixels
        )
        path.write_bytes(header + payload)

    def write_capture_v5(
        self,
        path: Path,
        irradiance_pixels: list[tuple[float, float, float, float]],
        world_pixels: list[tuple[float, float, float, float]],
        direct_light_pixels: list[tuple[float, float, float, float]],
    ) -> None:
        self.assertEqual(len(irradiance_pixels), len(world_pixels))
        self.assertEqual(len(irradiance_pixels), len(direct_light_pixels))
        header = HEADER_V5.pack(
            analyzer.MAGIC,
            5,
            len(irradiance_pixels),
            1,
            4,
            1,
            16,
            0,
            3,
            41,
            17,
            0xA11CE,
            9001,
            89,
            2,
            1,
            1,
            0,
            88,
            16,
            1,
            0,
            0.0125,
            0.025,
            0,
            len(irradiance_pixels),
        )
        payload = b"".join(
            analyzer.PIXEL.pack(*pixel)
            for pixel in irradiance_pixels + world_pixels + direct_light_pixels
        )
        path.write_bytes(header + payload)

    def test_loads_v5_direct_light_plane_without_breaking_v4(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            v4_path = Path(directory) / "capture-v4.rfirr"
            v5_path = Path(directory) / "capture-v5.rfirr"
            self.write_capture_v4(
                v4_path,
                [(0.1, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 0.0)],
            )
            direct_pixels = [(0.75, 0.5, 0.25, 1.0)]
            self.write_capture_v5(
                v5_path,
                [(0.1, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 0.0)],
                direct_pixels,
            )

            v4_capture = analyzer.load_capture(v4_path)
            v5_capture = analyzer.load_capture(v5_path)

        self.assertEqual(v4_capture.version, 4)
        self.assertEqual(v4_capture.plane_count, 2)
        self.assertEqual(v4_capture.direct_light_payload, b"")
        self.assertEqual(v5_capture.version, 5)
        self.assertEqual(v5_capture.plane_count, 3)
        self.assertEqual(
            list(analyzer.PIXEL.iter_unpack(v5_capture.direct_light_payload)),
            direct_pixels,
        )

    def test_cli_gates_independent_sunlit_and_shadowed_direct_light_rois(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "direct-light-evidence.rfirr"
            self.write_capture_v5(
                capture_path,
                [
                    (0.0, 0.0, 0.0, 1.0),
                    (0.0, 0.0, 0.0, 1.0),
                ],
                [
                    (1.0, 2.0, 3.0, 0.0),
                    (10.0, 2.0, 3.0, 1.0),
                ],
                [
                    (0.0, 0.0, 0.0, 1.0),
                    (1.0, 0.5, 0.25, 1.0),
                ],
            )
            common = (
                "--correctness",
                "--require-zero-rgb",
                "--direct-light-shadowed-roi",
                "0",
                "0",
                "0",
                "5",
                "5",
                "5",
                "--max-direct-light-shadowed-luminance-max",
                "0",
                "--direct-light-sunlit-roi",
                "9",
                "0",
                "0",
                "11",
                "5",
                "5",
                "--min-direct-light-sunlit-luminance-mean",
            )
            accepted = self.run_analyzer(capture_path, *common, "0.58")
            rejected = self.run_analyzer(capture_path, *common, "0.59")

        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        summary = json.loads(accepted.stdout)["capture"]
        self.assertTrue(summary["direct_light_available"])
        self.assertTrue(summary["direct_light_hit_mask_matches"])
        self.assertEqual(summary["direct_light_sunlit_roi_terrain_hit_count"], 1)
        self.assertAlmostEqual(
            summary["direct_light_sunlit_roi_luminance_mean"], 0.58825
        )
        self.assertEqual(
            summary["direct_light_shadowed_roi_luminance_max"], 0.0
        )
        self.assertEqual(rejected.returncode, 1, rejected.stderr)
        self.assertIn(
            "direct_light_sunlit_roi_luminance_mean: expected at least 0.59",
            json.loads(rejected.stdout)["validation_failures"][0],
        )

    def test_v5_bit_exact_comparison_includes_direct_light_plane(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first-v5.rfirr"
            second_path = Path(directory) / "second-v5.rfirr"
            irradiance = [(0.0, 0.0, 0.0, 1.0)]
            world = [(1.0, 2.0, 3.0, 1.0)]
            self.write_capture_v5(
                first_path,
                irradiance,
                world,
                [(0.75, 0.5, 0.25, 1.0)],
            )
            self.write_capture_v5(
                second_path,
                irradiance,
                world,
                [(0.5, 0.5, 0.25, 1.0)],
            )

            comparison = analyzer.compare(
                analyzer.load_capture(first_path),
                analyzer.load_capture(second_path),
            )

        self.assertTrue(comparison["compatible"])
        self.assertFalse(comparison["bit_exact"])
        self.assertNotEqual(
            comparison["first_direct_light_sha256"],
            comparison["second_direct_light_sha256"],
        )

    def test_loads_v4_canonical_field_source_and_build_identities(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "capture-v4.rfirr"
            self.write_capture_v4(
                capture_path,
                [(0.1, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 0.0)],
            )

            capture = analyzer.load_capture(capture_path)
            summary = analyzer.summarize(capture)

        self.assertEqual(capture.version, 4)
        self.assertEqual(capture.build_token_serial, 9001)
        self.assertEqual(capture.field_serial, 89)
        self.assertEqual(capture.source_field_serial, 88)
        self.assertEqual(capture.source_radiance_revision, 16)
        self.assertIsNone(capture.source_identity)
        self.assertEqual(summary["batch_order"], "forward")

    def test_v4_comparison_rejects_different_canonical_source(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first-v4.rfirr"
            second_path = Path(directory) / "second-v4.rfirr"
            pixels = [(0.1, 0.2, 0.3, 1.0)]
            world = [(1.0, 2.0, 3.0, 1.0)]
            self.write_capture_v4(first_path, pixels, world)
            self.write_capture_v4(
                second_path,
                pixels,
                world,
                source_field_serial=87,
            )

            comparison = analyzer.compare(
                analyzer.load_capture(first_path),
                analyzer.load_capture(second_path),
            )

        self.assertFalse(comparison["compatible"])
        self.assertIn("source_field_serial", comparison["metadata_mismatches"])

    def test_cli_requires_the_expected_batch_order(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "reverse.rfirr"
            self.write_capture_v4(
                capture_path,
                [(0.1, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 0.0)],
                batch_order=1,
            )

            accepted = self.run_analyzer(
                capture_path, "--expect-batch-order", "reverse"
            )
            rejected = self.run_analyzer(
                capture_path, "--expect-batch-order", "forward"
            )

        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)
        self.assertIn(
            "batch_order: expected forward, got reverse",
            json.loads(rejected.stdout)["validation_failures"],
        )

    def test_cli_requires_expected_capture_version_and_spacing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "spacing16-v4.rfirr"
            self.write_capture_v4(
                capture_path,
                [(0.1, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 0.0)],
            )

            accepted = self.run_analyzer(
                capture_path,
                "--expect-version",
                "4",
                "--expect-spacing-voxels",
                "16",
            )
            rejected = self.run_analyzer(
                capture_path,
                "--expect-version",
                "3",
                "--expect-spacing-voxels",
                "32",
            )

        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)
        failures = json.loads(rejected.stdout)["validation_failures"]
        self.assertIn("version: expected 3, got 4", failures)
        self.assertIn("spacing_voxels: expected 32, got 16", failures)

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

    def test_cli_applies_receiver_signal_channel_advantage_and_direct_sun_gates_to_world_roi(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "donor-receiver.rfirr"
            self.write_capture_v3(
                capture_path,
                [
                    (0.4, 0.1, 0.1, 1.0),
                    (0.2, 0.1, 0.0, 1.0),
                    (10.0, 10.0, 10.0, 1.0),
                ],
                [
                    (1.0, 2.0, 3.0, 0.0),
                    (2.0, 2.0, 3.0, 0.0),
                    (10.0, 2.0, 3.0, 1.0),
                ],
            )

            accepted = self.run_analyzer(
                capture_path,
                "--world-roi",
                "0",
                "0",
                "0",
                "5",
                "5",
                "5",
                "--roi-channel",
                "red",
                "--min-roi-channel-advantage",
                "0.19",
                "--min-roi-luminance-mean",
                "0.13",
                "--max-exact-direct-sun-visibility",
                "0",
            )
            rejected = self.run_analyzer(
                capture_path,
                "--world-roi",
                "0",
                "0",
                "0",
                "5",
                "5",
                "5",
                "--roi-channel",
                "red",
                "--min-roi-channel-advantage",
                "0.21",
            )

        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        summary = json.loads(accepted.stdout)["capture"]
        for actual, expected in zip(
            summary["world_roi_rgb_channel_mean"], [0.3, 0.1, 0.05]
        ):
            self.assertAlmostEqual(actual, expected)
        self.assertAlmostEqual(summary["world_roi_channel_advantage"][0], 0.2)
        self.assertAlmostEqual(summary["world_roi_luminance_mean"], 0.13891)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)

    def test_cli_gates_luminance_gain_over_a_compatible_world_roi_baseline(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            baseline_path = Path(directory) / "dogleg-s1.rfirr"
            current_path = Path(directory) / "dogleg-s2.rfirr"
            world = [(1.0, 2.0, 3.0, 0.0), (10.0, 2.0, 3.0, 1.0)]
            self.write_capture_v3(
                baseline_path,
                [(0.1, 0.1, 0.1, 1.0), (5.0, 5.0, 5.0, 1.0)],
                world,
                transport_stage=2,
                transport_iteration=1,
            )
            self.write_capture_v3(
                current_path,
                [(0.2, 0.2, 0.2, 1.0), (5.0, 5.0, 5.0, 1.0)],
                world,
                transport_stage=3,
                transport_iteration=2,
                source_stage=2,
                source_iteration=1,
            )
            common = (
                "--baseline",
                str(baseline_path),
                "--world-roi",
                "0",
                "0",
                "0",
                "5",
                "5",
                "5",
                "--min-roi-luminance-gain",
            )
            accepted = self.run_analyzer(current_path, *common, "0.09")
            rejected = self.run_analyzer(current_path, *common, "0.11")

        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        gain = json.loads(accepted.stdout)["baseline_comparison"]
        self.assertTrue(gain["compatible"])
        self.assertAlmostEqual(gain["baseline_roi_luminance_mean"], 0.1)
        self.assertAlmostEqual(gain["current_roi_luminance_mean"], 0.2)
        self.assertAlmostEqual(gain["roi_luminance_gain"], 0.1)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)

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

    def test_correctness_mode_rejects_non_converged_terminal_capture(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "non-converged.rfirr"
            self.write_capture_v3(
                capture_path,
                [(0.1, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 0.0)],
                transport_stage=5,
                transport_iteration=8,
                max_abs_delta=0.1,
                max_rel_delta=0.2,
            )

            diagnostic = self.run_analyzer(capture_path)
            correctness = self.run_analyzer(capture_path, "--correctness")

        self.assertEqual(diagnostic.returncode, 0, diagnostic.stderr)
        self.assertEqual(correctness.returncode, 1, correctness.stderr)
        self.assertIn(
            "correctness mode rejects NonConverged DDGI fields",
            json.loads(correctness.stdout)["validation_failures"],
        )

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
