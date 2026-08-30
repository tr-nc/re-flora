from __future__ import annotations

import json
import math
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
HEADER_V6 = HEADER_V4
HEADER_V7 = HEADER_V4
HEADER_V8 = HEADER_V4
HEADER_V9 = struct.Struct("<8s10I3Q4IQ3I2f2I4IQ4I11Q")
HEADER_V10 = struct.Struct("<8s10I3Q4IQ3I2f2I4IQ8I13Q")


class AnalyzeEnvironmentIrradianceCaptureTests(unittest.TestCase):
    def test_published_v9_golden_keeps_the_252_byte_layout(self) -> None:
        fixture_hex = (
            Path(__file__).with_name("fixtures") / "ddgi_filter_evidence_v9.hex"
        ).read_text()
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "published-v9.rfirr"
            capture_path.write_bytes(bytes.fromhex(fixture_hex))
            capture = analyzer.load_capture(capture_path)

        self.assertEqual(capture.version, 9)
        self.assertEqual(analyzer.HEADER_V9.size, 252)
        self.assertEqual(capture.filter_evidence["irradiance_history"]["owner_version"], 1)

    def test_rust_producer_v10_golden_decodes_with_exact_filter_witness(self) -> None:
        fixture_hex = (
            Path(__file__).with_name("fixtures") / "ddgi_filter_evidence_v10.hex"
        ).read_text()
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "rust-producer-v10.rfirr"
            capture_path.write_bytes(bytes.fromhex(fixture_hex))
            capture = analyzer.load_capture(capture_path)

        self.assertEqual(capture.version, 10)
        self.assertEqual(capture.update_epoch, 6)
        self.assertEqual(capture.grid_dimensions, (1, 2, 2))
        self.assertEqual(capture.configured_history_retention_q16, 64_881)
        evidence = capture.filter_evidence
        self.assertIsNotNone(evidence)
        assert evidence is not None
        self.assertEqual(evidence["irradiance_history"]["owner_version_mask"], 2)
        self.assertEqual(
            evidence["irradiance_history"]["blend_retention_q16_sum"],
            112_348,
        )
        self.assertEqual(
            evidence["irradiance_history"]["blend_retention_q16_max"],
            56_174,
        )

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

    def run_compatibility_analyzer(
        self, version: int, capture: Path, *arguments: str
    ) -> subprocess.CompletedProcess[str]:
        return self.run_analyzer(
            capture, "--expect-version", str(version), *arguments
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

    def write_capture_v6(
        self,
        path: Path,
        irradiance_pixels: list[tuple[float, float, float, float]],
        world_pixels: list[tuple[float, float, float, float]],
        direct_light_pixels: list[tuple[float, float, float, float]],
        *,
        build_token_serial: int = 9001,
        field_serial: int = 89,
        update_epoch: int = 31,
        source_field_serial: int = 88,
    ) -> None:
        self.assertEqual(len(irradiance_pixels), len(world_pixels))
        self.assertEqual(len(irradiance_pixels), len(direct_light_pixels))
        header = HEADER_V6.pack(
            analyzer.MAGIC,
            6,
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
            build_token_serial,
            field_serial,
            2,
            update_epoch,
            1,
            update_epoch - 1,
            source_field_serial,
            17,
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

    def write_capture_v7(
        self,
        path: Path,
        irradiance_pixels: list[tuple[float, float, float, float]],
        world_pixels: list[tuple[float, float, float, float]],
        direct_light_pixels: list[tuple[float, float, float, float]],
        receiver_pixels: list[tuple[float, float, float, float]],
    ) -> None:
        self.assertEqual(len(irradiance_pixels), len(world_pixels))
        self.assertEqual(len(irradiance_pixels), len(direct_light_pixels))
        self.assertEqual(len(irradiance_pixels), len(receiver_pixels))
        header = HEADER_V7.pack(
            analyzer.MAGIC,
            7,
            len(irradiance_pixels),
            1,
            4,
            1,
            16,
            0,
            4,
            41,
            17,
            0xA11CE,
            9001,
            89,
            2,
            31,
            1,
            30,
            88,
            17,
            1,
            0,
            0.0125,
            0.025,
            0,
            len(irradiance_pixels),
        )
        payload = b"".join(
            analyzer.PIXEL.pack(*pixel)
            for pixel in (
                irradiance_pixels
                + world_pixels
                + direct_light_pixels
                + receiver_pixels
            )
        )
        path.write_bytes(header + payload)

    def write_capture_v8(
        self,
        path: Path,
        irradiance_pixels: list[tuple[float, float, float, float]],
        world_pixels: list[tuple[float, float, float, float]],
        direct_light_pixels: list[tuple[float, float, float, float]],
        receiver_pixels: list[tuple[float, float, float, float]],
        direct_sun_shadow_pixels: list[tuple[float, float, float, float]],
        *,
        debug_view: int = 0,
    ) -> None:
        pixel_count = len(irradiance_pixels)
        self.assertTrue(
            all(
                len(pixels) == pixel_count
                for pixels in (
                    world_pixels,
                    direct_light_pixels,
                    receiver_pixels,
                    direct_sun_shadow_pixels,
                )
            )
        )
        header = HEADER_V8.pack(
            analyzer.MAGIC,
            8,
            pixel_count,
            1,
            4,
            1,
            16,
            debug_view,
            5,
            41,
            17,
            0xA11CE,
            9001,
            89,
            2,
            31,
            1,
            30,
            88,
            17,
            1,
            0,
            0.0125,
            0.025,
            0,
            pixel_count,
        )
        payload = b"".join(
            analyzer.PIXEL.pack(*pixel)
            for pixel in (
                irradiance_pixels
                + world_pixels
                + direct_light_pixels
                + receiver_pixels
                + direct_sun_shadow_pixels
            )
        )
        path.write_bytes(header + payload)

    def write_capture_v10(
        self,
        path: Path,
        *,
        debug_view: int = 0,
        grid_dimensions: tuple[int, int, int] = (1, 2, 2),
        configured_retention_q16: int = 64_881,
        visibility_samples: int = 128,
        visibility_accept: int = 80,
        visibility_reject: int = 48,
        irradiance_retention_sum_q16: int = 65_536,
        irradiance_retention_max_q16: int = 32_768,
        visibility_retention_sum_q16: int = 65_536,
        visibility_retention_max_q16: int = 32_768,
        update_epoch: int = 1,
    ) -> None:
        voxel = 1.0 / 256.0
        header = HEADER_V10.pack(
            analyzer.MAGIC,
            10,
            1,
            1,
            4,
            1,
            16,
            debug_view,
            5,
            41,
            17,
            0xA11CE,
            9001,
            89,
            1,
            update_epoch,
            1,
            0,
            88,
            17,
            1,
            0,
            0.0125,
            0.025,
            0,
            1,
            1,
            2,
            2,
            2,
            89,
            update_epoch,
            4,
            1,
            0,
            *grid_dimensions,
            configured_retention_q16,
            0,
            2,
            2,
            irradiance_retention_sum_q16,
            irradiance_retention_max_q16,
            0,
            2,
            2,
            visibility_retention_sum_q16,
            visibility_retention_max_q16,
            visibility_samples,
            visibility_accept,
            visibility_reject,
        )
        payload = b"".join(
            analyzer.PIXEL.pack(*pixel)
            for pixel in (
                (0.25, 0.5, 0.75, 1.0),
                (1.0, 2.0, 3.0, 0.0),
                (0.0, 0.0, 0.0, 1.0),
                (0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 1.0),
                (1.0, 1.0, 1.0, 1.0),
            )
        )
        path.write_bytes(header + payload)

    def test_v10_filter_evidence_is_typed_and_debug_view_independent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "filter-evidence.rfirr"
            self.write_capture_v10(capture_path, debug_view=22)
            result = self.run_analyzer(
                capture_path,
                "--expect-debug-view",
                "moment-support",
                "--require-filter-history-retain-blend",
                "--expect-filter-blend-retention-q16",
                "32768",
                "--min-filter-visibility-reject-count",
                "1",
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        capture = json.loads(result.stdout)["capture"]
        self.assertEqual(capture["debug_view"], "moment-support")
        self.assertEqual(capture["filter_evidence"]["field_serial"], 89)
        self.assertEqual(capture["filter_evidence"]["update_epoch"], 1)
        self.assertEqual(
            capture["filter_evidence"]["irradiance_history"]["blend"], 2
        )
        self.assertEqual(
            capture["filter_evidence"]["irradiance_history"][
                "blend_retention_q16_max"
            ],
            32_768,
        )
        self.assertEqual(
            capture["filter_evidence"]["visibility_samples"]["reject"], 48
        )

    def test_v10_filter_evidence_rejects_an_invalid_sample_partition(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "invalid-filter-evidence.rfirr"
            self.write_capture_v10(
                capture_path,
                visibility_samples=128,
                visibility_accept=80,
                visibility_reject=49,
            )
            with self.assertRaisesRegex(ValueError, "visibility sample partition"):
                analyzer.load_capture(capture_path)

    def test_v10_filter_evidence_requires_the_authoritative_grid_product(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "grid-mismatch.rfirr"
            self.write_capture_v10(capture_path, grid_dimensions=(1, 1, 3))
            with self.assertRaisesRegex(ValueError, "grid product"):
                analyzer.load_capture(capture_path)

    def test_v10_visibility_samples_reject_under_over_and_partial_probe_counts(self) -> None:
        mutations = (
            (64, 40, 24, "undercounts Blend probes"),
            (192, 120, 72, "exceeds fresh history probes"),
            (127, 80, 47, "whole 64-ray probes"),
        )
        for samples, accept, reject, message in mutations:
            with self.subTest(samples=samples), tempfile.TemporaryDirectory() as directory:
                capture_path = Path(directory) / "sample-completeness.rfirr"
                self.write_capture_v10(
                    capture_path,
                    visibility_samples=samples,
                    visibility_accept=accept,
                    visibility_reject=reject,
                )
                with self.assertRaisesRegex(ValueError, message):
                    analyzer.load_capture(capture_path)

    def test_v10_filter_evidence_rejects_an_average_only_retention_witness(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "mixed-retention-filter-evidence.rfirr"
            self.write_capture_v10(
                capture_path,
                irradiance_retention_sum_q16=65_536,
                irradiance_retention_max_q16=65_536,
            )
            with self.assertRaisesRegex(ValueError, "exact Blend retention"):
                analyzer.load_capture(capture_path)

    def test_v10_local_recovery_retention_is_derived_from_the_capture_epoch(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "local-recovery-e8.rfirr"
            self.write_capture_v10(
                capture_path,
                update_epoch=8,
                irradiance_retention_sum_q16=116_508,
                irradiance_retention_max_q16=58_254,
                visibility_retention_sum_q16=116_508,
                visibility_retention_max_q16=58_254,
            )
            accepted = self.run_analyzer(
                capture_path,
                "--require-filter-local-recovery-policy",
            )

        self.assertEqual(accepted.returncode, 0, accepted.stderr)

    def test_v10_local_recovery_retention_is_capped_by_configured_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "local-recovery-low-h.rfirr"
            self.write_capture_v10(
                capture_path,
                update_epoch=8,
                configured_retention_q16=16_384,
                irradiance_retention_sum_q16=32_768,
                irradiance_retention_max_q16=16_384,
                visibility_retention_sum_q16=32_768,
                visibility_retention_max_q16=16_384,
            )
            accepted = self.run_analyzer(
                capture_path,
                "--require-filter-local-recovery-policy",
            )

        self.assertEqual(accepted.returncode, 0, accepted.stderr)

    def test_cli_names_and_checks_extended_ddgi_debug_views(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "unoccluded.rfirr"
            voxel = 1.0 / 256.0
            self.write_capture_v8(
                capture_path,
                [(0.4, 0.4, 0.4, 1.0)],
                [(1.0, 2.0, 3.0, 0.0)],
                [(0.0, 0.0, 0.0, 1.0)],
                [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 1.0)],
                [(1.0, 1.0, 1.0, 1.0)],
                debug_view=12,
            )
            accepted = self.run_compatibility_analyzer(
                8,
                capture_path,
                "--expect-debug-view",
                "unoccluded-irradiance",
            )
            rejected = self.run_compatibility_analyzer(
                8,
                capture_path,
                "--expect-debug-view",
                "equal-weight-irradiance",
            )

        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        self.assertEqual(
            json.loads(accepted.stdout)["capture"]["debug_view"],
            "unoccluded-irradiance",
        )
        self.assertEqual(rejected.returncode, 1, rejected.stderr)

    def test_cli_names_moment_support_diagnostic(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "moment-support.rfirr"
            voxel = 1.0 / 256.0
            self.write_capture_v8(
                capture_path,
                [(0.5, 0.25, 0.2, 1.0)],
                [(1.0, 2.0, 3.0, 0.0)],
                [(0.0, 0.0, 0.0, 1.0)],
                [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 1.0)],
                [(1.0, 1.0, 1.0, 1.0)],
                debug_view=22,
            )
            result = self.run_compatibility_analyzer(
                8,
                capture_path,
                "--expect-debug-view",
                "moment-support",
            )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout)["capture"]["debug_view"],
            "moment-support",
        )

    def test_cli_requires_visibility_routes_to_have_observable_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            moment_path = Path(directory) / "moment.rfirr"
            exact_path = Path(directory) / "exact.rfirr"
            voxel = 1.0 / 256.0
            common_planes = (
                [(1.0, 2.0, 3.0, 0.0)],
                [(0.0, 0.0, 0.0, 1.0)],
                [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 1.0)],
                [(1.0, 1.0, 1.0, 1.0)],
            )
            self.write_capture_v8(
                moment_path,
                [(0.8, 0.8, 0.8, 1.0)],
                *common_planes,
                debug_view=1,
            )
            self.write_capture_v8(
                exact_path,
                [(0.2, 0.2, 0.2, 1.0)],
                *common_planes,
                debug_view=2,
            )
            accepted = self.run_compatibility_analyzer(
                8,
                moment_path,
                "--reference",
                str(exact_path),
                "--min-reference-error-p99",
                "0.59",
            )
            rejected = self.run_compatibility_analyzer(
                8,
                moment_path,
                "--reference",
                str(exact_path),
                "--min-reference-error-p99",
                "0.61",
            )

        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)

    def test_reference_difference_gate_accepts_signed_luminance_reversal(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            darker_path = Path(directory) / "darker.rfirr"
            brighter_path = Path(directory) / "brighter.rfirr"
            voxel = 1.0 / 256.0
            common_planes = (
                [(1.0, 2.0, 3.0, 0.0)],
                [(0.0, 0.0, 0.0, 1.0)],
                [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 1.0)],
                [(1.0, 1.0, 1.0, 1.0)],
            )
            self.write_capture_v8(
                darker_path,
                [(0.1, 0.1, 0.1, 1.0)],
                *common_planes,
                debug_view=12,
            )
            self.write_capture_v8(
                brighter_path,
                [(0.5, 0.5, 0.5, 1.0)],
                *common_planes,
                debug_view=0,
            )

            accepted = self.run_compatibility_analyzer(
                8,
                darker_path,
                "--reference",
                str(brighter_path),
                "--min-reference-error-p99",
                "0.39",
            )

        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        comparison = json.loads(accepted.stdout)["reference_comparison"]
        self.assertAlmostEqual(comparison["luminance_error_p99"], 0.4)

    def test_cli_reports_failed_reference_ceiling_gates(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first.rfirr"
            reference_path = Path(directory) / "reference.rfirr"
            voxel = 1.0 / 256.0
            common_planes = (
                [(1.0, 2.0, 3.0, 0.0)],
                [(0.0, 0.0, 0.0, 1.0)],
                [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 1.0)],
                [(1.0, 1.0, 1.0, 1.0)],
            )
            self.write_capture_v8(
                first_path,
                [(0.8, 0.8, 0.8, 1.0)],
                *common_planes,
            )
            self.write_capture_v8(
                reference_path,
                [(0.2, 0.2, 0.2, 1.0)],
                *common_planes,
            )
            rejected = self.run_compatibility_analyzer(
                8,
                first_path,
                "--reference",
                str(reference_path),
                "--max-reference-error-p99",
                "0.5",
                "--max-reference-overestimate-p99",
                "0.5",
            )

        self.assertEqual(rejected.returncode, 1, rejected.stderr)
        failures = json.loads(rejected.stdout)["validation_failures"]
        self.assertIn(
            "reference luminance_error_p99: expected at most 0.5, got 0.6",
            failures,
        )
        self.assertIn(
            "reference luminance_overestimate_p99: expected at most 0.5, got 0.6",
            failures,
        )

    def test_reference_gates_require_reference_capture(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "capture.rfirr"
            self.write_capture(capture_path, [(0.1, 0.2, 0.3, 1.0)])

            for option in (
                "--min-reference-error-p99",
                "--max-reference-error-p99",
                "--max-reference-overestimate-p99",
                "--max-reference-error-max",
            ):
                with self.subTest(option=option):
                    rejected = self.run_compatibility_analyzer(
                        2, capture_path, option, "0.1"
                    )
                    self.assertEqual(rejected.returncode, 1, rejected.stderr)
                    self.assertIn(
                        f"{option} requires --reference",
                        json.loads(rejected.stdout)["validation_failures"],
                    )

    def test_incompatible_reference_with_ceiling_returns_failure_json(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first.rfirr"
            reference_path = Path(directory) / "reference.rfirr"
            self.write_capture(first_path, [(0.1, 0.2, 0.3, 1.0)])
            self.write_capture(
                reference_path,
                [(0.1, 0.2, 0.3, 1.0), (0.1, 0.2, 0.3, 1.0)],
            )

            rejected = self.run_compatibility_analyzer(
                2,
                first_path,
                "--reference",
                str(reference_path),
                "--max-reference-error-p99",
                "0.1",
            )

        self.assertEqual(rejected.returncode, 1, rejected.stderr)
        self.assertNotIn("Traceback", rejected.stderr)
        self.assertIn(
            "reference comparison is incompatible",
            json.loads(rejected.stdout)["validation_failures"],
        )

    def test_legacy_capture_pair_cannot_satisfy_reference_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first-v2.rfirr"
            reference_path = Path(directory) / "reference-v2.rfirr"
            pixels = [(0.1, 0.2, 0.3, 1.0)]
            self.write_capture(first_path, pixels)
            self.write_capture(reference_path, pixels)

            rejected = self.run_compatibility_analyzer(
                2, first_path, "--reference", str(reference_path)
            )

        self.assertEqual(rejected.returncode, 1, rejected.stderr)
        report = json.loads(rejected.stdout)
        self.assertIn(
            "reference comparison requires RFIRR v8-v10 five-plane identity evidence",
            report["validation_failures"],
        )
        self.assertFalse(
            report["reference_comparison"]["identity_planes_available"]
        )

    def test_reference_requires_finite_planes_and_exact_world_coordinates(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first.rfirr"
            nonfinite_path = Path(directory) / "nonfinite-reference.rfirr"
            moved_path = Path(directory) / "moved-reference.rfirr"
            different_hit_path = Path(directory) / "different-hit-reference.rfirr"
            voxel = 1.0 / 256.0
            irradiance = [(0.2, 0.2, 0.2, 1.0)]
            world = [(1.0, 2.0, 3.0, 0.0)]
            direct = [(0.0, 0.0, 0.0, 1.0)]
            receiver = [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 1.0)]
            shadows = [(1.0, 1.0, 1.0, 1.0)]
            self.write_capture_v8(
                first_path, irradiance, world, direct, receiver, shadows
            )
            self.write_capture_v8(
                nonfinite_path,
                irradiance,
                world,
                [(math.nan, 0.0, 0.0, 1.0)],
                receiver,
                shadows,
            )
            self.write_capture_v8(
                moved_path,
                irradiance,
                [(1.0 + voxel, 2.0, 3.0, 0.0)],
                direct,
                receiver,
                shadows,
            )
            self.write_capture_v8(
                different_hit_path,
                [(0.2, 0.2, 0.2, 0.0)],
                world,
                direct,
                receiver,
                shadows,
            )

            nonfinite = self.run_compatibility_analyzer(
                8,
                first_path, "--reference", str(nonfinite_path)
            )
            moved = self.run_compatibility_analyzer(
                8, first_path, "--reference", str(moved_path)
            )
            different_hit = self.run_compatibility_analyzer(
                8, first_path, "--reference", str(different_hit_path)
            )

        self.assertEqual(nonfinite.returncode, 1, nonfinite.stderr)
        self.assertIn(
            "reference capture contains non-finite required-plane values",
            json.loads(nonfinite.stdout)["validation_failures"],
        )
        self.assertEqual(moved.returncode, 1, moved.stderr)
        self.assertIn(
            "reference world XYZ payload does not match capture",
            json.loads(moved.stdout)["validation_failures"],
        )
        self.assertEqual(different_hit.returncode, 1, different_hit.stderr)
        self.assertIn(
            "reference terrain hit mask does not match capture",
            json.loads(different_hit.stdout)["validation_failures"],
        )

    def test_reference_max_gate_rejects_a_small_tail_outlier(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first.rfirr"
            reference_path = Path(directory) / "reference.rfirr"
            pixels = [(0.0, 0.0, 0.0, 1.0)] * 100
            reference_pixels = list(pixels)
            reference_pixels[-1] = (1.0, 1.0, 1.0, 1.0)
            voxel = 1.0 / 256.0
            world = [(1.0, 2.0, 3.0, 0.0)] * 100
            direct = [(0.0, 0.0, 0.0, 1.0)] * 100
            receiver = [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 1.0)] * 100
            shadows = [(1.0, 1.0, 1.0, 1.0)] * 100
            self.write_capture_v8(
                first_path, pixels, world, direct, receiver, shadows
            )
            self.write_capture_v8(
                reference_path,
                reference_pixels,
                world,
                direct,
                receiver,
                shadows,
            )

            rejected = self.run_compatibility_analyzer(
                2,
                first_path,
                "--reference",
                str(reference_path),
                "--max-reference-error-p99",
                "0.02",
                "--max-reference-error-max",
                "0.5",
            )

        self.assertEqual(rejected.returncode, 1, rejected.stderr)
        self.assertIn(
            "reference luminance_error_max: expected at most 0.5, got 1",
            json.loads(rejected.stdout)["validation_failures"],
        )

    def test_cli_gates_debug_route_roi_gain_against_real_capture_baseline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            final_path = Path(directory) / "final.rfirr"
            unoccluded_path = Path(directory) / "unoccluded.rfirr"
            voxel = 1.0 / 256.0
            common_planes = (
                [(1.0, 2.0, 3.0, 0.0)],
                [(0.0, 0.0, 0.0, 1.0)],
                [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 1.0)],
                [(1.0, 1.0, 1.0, 1.0)],
            )
            self.write_capture_v8(
                final_path,
                [(0.1, 0.1, 0.1, 1.0)],
                *common_planes,
                debug_view=0,
            )
            self.write_capture_v8(
                unoccluded_path,
                [(0.5, 0.5, 0.5, 1.0)],
                *common_planes,
                debug_view=12,
            )
            common = (
                "--debug-baseline",
                str(final_path),
                "--world-roi",
                "0",
                "0",
                "0",
                "2",
                "3",
                "4",
                "--min-debug-roi-luminance-gain",
            )
            accepted = self.run_compatibility_analyzer(
                8, unoccluded_path, *common, "0.39"
            )
            rejected = self.run_compatibility_analyzer(
                8, unoccluded_path, *common, "0.41"
            )

        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        report = json.loads(accepted.stdout)["debug_baseline_comparison"]
        self.assertTrue(report["compatible"])
        self.assertAlmostEqual(report["roi_luminance_gain"], 0.4)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)

    def test_loads_v6_lifecycle_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture-v6.rfirr"
            self.write_capture_v6(
                path,
                [(0.1, 0.2, 0.3, 1.0)],
                [(0.0, 0.0, 0.0, 0.0)],
                [(0.0, 0.0, 0.0, 1.0)],
            )
            result = self.run_analyzer(
                path,
                "--expect-version",
                "6",
                "--expect-lifecycle-state",
                "converged",
                "--expect-update-epoch",
                "31",
                "--expect-source-state",
                "converging",
                "--expect-source-update-epoch",
                "30",
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        summary = json.loads(result.stdout)["capture"]
        self.assertEqual(summary["lifecycle_state"], "converged")
        self.assertEqual(summary["update_epoch"], 31)
        self.assertEqual(summary["source_state"], "converging")
        self.assertEqual(summary["source_update_epoch"], 30)

    def test_v6_cross_process_comparison_ignores_only_process_local_serials(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first-v6.rfirr"
            second_path = Path(directory) / "second-v6.rfirr"
            different_epoch_path = Path(directory) / "different-epoch-v6.rfirr"
            irradiance = [(0.1, 0.2, 0.3, 1.0)]
            world = [(1.0, 2.0, 3.0, 0.0)]
            direct = [(0.4, 0.5, 0.6, 1.0)]
            self.write_capture_v6(first_path, irradiance, world, direct)
            self.write_capture_v6(
                second_path,
                irradiance,
                world,
                direct,
                build_token_serial=9100,
                field_serial=109,
                source_field_serial=108,
            )
            self.write_capture_v6(
                different_epoch_path,
                irradiance,
                world,
                direct,
                build_token_serial=9200,
                field_serial=119,
                update_epoch=32,
                source_field_serial=118,
            )

            first = analyzer.load_capture(first_path)
            second = analyzer.load_capture(second_path)
            comparison = analyzer.compare(first, second)
            reference = analyzer.compare_reference(first, second)
            different_epoch = analyzer.compare(
                first, analyzer.load_capture(different_epoch_path)
            )

        self.assertTrue(comparison["compatible"])
        self.assertTrue(comparison["bit_exact"])
        self.assertEqual(
            comparison["process_local_identity_mismatches"],
            ["build_token_serial", "field_serial", "source_field_serial"],
        )
        self.assertFalse(reference["compatible"])
        self.assertFalse(reference["identity_planes_available"])
        self.assertFalse(different_epoch["compatible"])
        self.assertIn("update_epoch", different_epoch["metadata_mismatches"])

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
            accepted = self.run_compatibility_analyzer(
                5, capture_path, *common, "0.58"
            )
            rejected = self.run_compatibility_analyzer(
                5, capture_path, *common, "0.59"
            )

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

    def test_v5_comparison_reports_and_optionally_gates_direct_light_plane(self) -> None:
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
            environment_only = self.run_compatibility_analyzer(
                5, first_path, "--compare", str(second_path)
            )
            including_direct = self.run_compatibility_analyzer(
                5,
                first_path,
                "--compare",
                str(second_path),
                "--compare-direct-light",
            )

        self.assertTrue(comparison["compatible"])
        self.assertTrue(comparison["environment_bit_exact"])
        self.assertFalse(comparison["direct_light_bit_exact"])
        self.assertFalse(comparison["bit_exact"])
        self.assertNotEqual(
            comparison["first_direct_light_sha256"],
            comparison["second_direct_light_sha256"],
        )
        self.assertEqual(environment_only.returncode, 0, environment_only.stderr)
        self.assertEqual(including_direct.returncode, 1, including_direct.stderr)
        self.assertIn(
            "comparison direct-light plane is not bit-exact",
            json.loads(including_direct.stdout)["validation_failures"],
        )

    def test_comparison_reports_environment_bit_exact_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first.rfirr"
            second_path = Path(directory) / "second.rfirr"
            self.write_capture(first_path, [(0.1, 0.2, 0.3, 1.0)])
            self.write_capture(second_path, [(0.1, 0.2, 0.4, 1.0)])

            result = self.run_analyzer(
                first_path, "--compare", str(second_path)
            )

        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn(
            "comparison environment irradiance and terrain hit-mask plane "
            "is not bit-exact",
            json.loads(result.stdout)["validation_failures"],
        )

    def test_comparison_reports_metadata_without_blaming_equal_payloads(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first-v6.rfirr"
            second_path = Path(directory) / "second-v6.rfirr"
            irradiance = [(0.1, 0.2, 0.3, 1.0)]
            world = [(1.0, 2.0, 3.0, 0.0)]
            direct = [(0.4, 0.5, 0.6, 1.0)]
            self.write_capture_v6(first_path, irradiance, world, direct)
            self.write_capture_v6(
                second_path,
                irradiance,
                world,
                direct,
                update_epoch=32,
            )

            result = self.run_compatibility_analyzer(
                6,
                first_path,
                "--compare",
                str(second_path),
                "--compare-direct-light",
            )

        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(
            report["comparison"]["metadata_mismatches"],
            ["update_epoch", "source_update_epoch"],
        )
        self.assertTrue(report["comparison"]["direct_light_payload_bit_exact"])
        self.assertEqual(
            report["validation_failures"],
            [
                "comparison capture identity is incompatible: base_mismatches=[] "
                "metadata_mismatches=['update_epoch', 'source_update_epoch']"
            ],
        )

    def test_comparison_reports_shadow_plane_without_blaming_environment(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first-v8.rfirr"
            second_path = Path(directory) / "second-v8.rfirr"
            voxel = 1.0 / 256.0
            planes = (
                [(0.1, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 0.0)],
                [(0.4, 0.5, 0.6, 1.0)],
            )
            self.write_capture_v8(
                first_path,
                *planes,
                [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 1.0)],
                [(1.0, 1.0, 1.0, 1.0)],
            )
            self.write_capture_v8(
                second_path,
                *planes,
                [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 0.75)],
                [(1.0, 1.0, 1.0, 1.0)],
            )

            result = self.run_compatibility_analyzer(
                8, first_path, "--compare", str(second_path)
            )

        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads(result.stdout)
        self.assertTrue(report["comparison"]["environment_irradiance_bit_exact"])
        self.assertTrue(report["comparison"]["world_payload_bit_exact"])
        self.assertFalse(report["comparison"]["terrain_shadow_receiver_bit_exact"])
        self.assertEqual(
            report["validation_failures"],
            ["comparison terrain-shadow receiver plane is not bit-exact"],
        )

    def test_comparison_names_world_alpha_as_exact_sun_visibility(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            first_path = Path(directory) / "first-v8.rfirr"
            second_path = Path(directory) / "second-v8.rfirr"
            voxel = 1.0 / 256.0
            irradiance = [(0.1, 0.2, 0.3, 1.0)]
            direct = [(0.4, 0.5, 0.6, 1.0)]
            receiver = [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 1.0)]
            shadows = [(1.0, 1.0, 1.0, 1.0)]
            self.write_capture_v8(
                first_path,
                irradiance,
                [(1.0, 2.0, 3.0, 0.0)],
                direct,
                receiver,
                shadows,
            )
            self.write_capture_v8(
                second_path,
                irradiance,
                [(1.0, 2.0, 3.0, 1.0)],
                direct,
                receiver,
                shadows,
            )

            result = self.run_compatibility_analyzer(
                8, first_path, "--compare", str(second_path)
            )

        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads(result.stdout)
        self.assertFalse(report["comparison"]["world_payload_bit_exact"])
        self.assertEqual(
            report["validation_failures"],
            [
                "comparison world XYZ and exact-sun-visibility plane "
                "is not bit-exact"
            ],
        )

    def test_cli_gates_direct_light_roi_delta_from_environment_identical_baseline(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            baseline_path = Path(directory) / "baseline.rfirr"
            changed_path = Path(directory) / "changed.rfirr"
            irradiance = [(0.1, 0.2, 0.3, 1.0)]
            baseline_world = [(1.0, 2.0, 3.0, 0.0)]
            changed_world = [(1.0, 2.0, 3.0, 1.0)]
            self.write_capture_v5(
                baseline_path,
                irradiance,
                baseline_world,
                [(0.1, 0.1, 0.1, 1.0)],
            )
            self.write_capture_v5(
                changed_path,
                irradiance,
                changed_world,
                [(0.4, 0.4, 0.4, 1.0)],
            )
            common = (
                "--radiance-frame-baseline",
                str(baseline_path),
                "--direct-light-baseline",
                str(baseline_path),
                "--direct-light-sunlit-roi",
                "0",
                "0",
                "0",
                "2",
                "3",
                "4",
                "--min-direct-light-sunlit-roi-luminance-absolute-delta",
            )
            accepted = self.run_compatibility_analyzer(
                5, changed_path, *common, "0.29"
            )
            rejected = self.run_compatibility_analyzer(
                5, changed_path, *common, "0.31"
            )

        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        report = json.loads(accepted.stdout)
        self.assertTrue(
            report["radiance_frame_comparison"]["environment_payload_bit_exact"]
        )
        self.assertTrue(report["radiance_frame_comparison"]["world_xyz_bit_exact"])
        self.assertTrue(
            report["radiance_frame_comparison"]["terrain_hit_mask_bit_exact"]
        )
        self.assertFalse(
            report["radiance_frame_comparison"]["exact_sun_visibility_bit_exact"]
        )
        self.assertAlmostEqual(
            report["direct_light_baseline_comparison"][
                "sunlit_roi_luminance_absolute_delta"
            ],
            0.3,
        )
        self.assertEqual(rejected.returncode, 1, rejected.stderr)

    def test_v6_direct_light_baseline_comparison(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            baseline_path = Path(directory) / "baseline-v6.rfirr"
            changed_path = Path(directory) / "changed-v6.rfirr"
            irradiance = [(0.1, 0.2, 0.3, 1.0)]
            world = [(1.0, 2.0, 3.0, 0.0)]
            self.write_capture_v6(
                baseline_path,
                irradiance,
                world,
                [(0.1, 0.1, 0.1, 1.0)],
            )
            self.write_capture_v6(
                changed_path,
                irradiance,
                world,
                [(0.4, 0.4, 0.4, 1.0)],
            )

            comparison = analyzer.compare_direct_light_baseline(
                analyzer.load_capture(changed_path),
                analyzer.load_capture(baseline_path),
                (0.0, 0.0, 0.0, 2.0, 3.0, 4.0),
            )

        self.assertTrue(comparison["compatible"])
        self.assertAlmostEqual(
            comparison["sunlit_roi_luminance_absolute_delta"], 0.3
        )

    def test_radiance_frame_compares_terrain_hit_mask_independently(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            baseline_path = Path(directory) / "baseline.rfirr"
            same_mask_path = Path(directory) / "same-mask.rfirr"
            changed_mask_path = Path(directory) / "changed-mask.rfirr"
            world = [(1.0, 2.0, 3.0, 0.0)]
            direct = [(0.1, 0.1, 0.1, 1.0)]
            self.write_capture_v5(
                baseline_path,
                [(0.1, 0.2, 0.3, 1.0)],
                world,
                direct,
            )
            self.write_capture_v5(
                same_mask_path,
                [(0.4, 0.5, 0.6, 1.0)],
                world,
                direct,
            )
            self.write_capture_v5(
                changed_mask_path,
                [(0.1, 0.2, 0.3, 0.0)],
                world,
                direct,
            )

            baseline = analyzer.load_capture(baseline_path)
            same_mask = analyzer.compare_radiance_frame(
                analyzer.load_capture(same_mask_path), baseline
            )
            changed_mask = analyzer.compare_radiance_frame(
                analyzer.load_capture(changed_mask_path), baseline
            )

        self.assertFalse(same_mask["environment_payload_bit_exact"])
        self.assertTrue(same_mask["terrain_hit_mask_bit_exact"])
        self.assertFalse(changed_mask["environment_payload_bit_exact"])
        self.assertFalse(changed_mask["terrain_hit_mask_bit_exact"])

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

            accepted = self.run_compatibility_analyzer(
                4, capture_path, "--expect-batch-order", "reverse"
            )
            rejected = self.run_compatibility_analyzer(
                4, capture_path, "--expect-batch-order", "forward"
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

    def test_cli_defaults_to_current_and_requires_explicit_compatibility(self) -> None:
        v10_fixture_hex = (
            Path(__file__).with_name("fixtures") / "ddgi_filter_evidence_v10.hex"
        ).read_text()
        v9_fixture_hex = (
            Path(__file__).with_name("fixtures") / "ddgi_filter_evidence_v9.hex"
        ).read_text()
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "current-v10.rfirr"
            capture_path.write_bytes(bytes.fromhex(v10_fixture_hex))
            stale_path = Path(directory) / "published-v9.rfirr"
            stale_path.write_bytes(bytes.fromhex(v9_fixture_hex))

            current_default = self.run_analyzer(capture_path)
            stale_default = self.run_analyzer(stale_path)
            explicit_compatibility = self.run_analyzer(
                stale_path, "--expect-version", "9"
            )
            explicit_current = self.run_analyzer(
                capture_path, "--expect-version", "current"
            )

        self.assertEqual(current_default.returncode, 0, current_default.stderr)
        self.assertEqual(
            json.loads(current_default.stdout)["capture"]["version"],
            analyzer.CURRENT_RFIRR_VERSION,
        )
        self.assertEqual(stale_default.returncode, 1, stale_default.stderr)
        self.assertIn(
            "version: expected 10, got 9",
            json.loads(stale_default.stdout)["validation_failures"],
        )
        self.assertEqual(
            explicit_compatibility.returncode, 0, explicit_compatibility.stderr
        )
        self.assertEqual(explicit_current.returncode, 0, explicit_current.stderr)

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

    def test_world_roi_zero_counts_distinguish_environment_and_combined_light(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "tree-branch-black.rfirr"
            self.write_capture_v5(
                capture_path,
                [
                    (0.0, 0.0, 0.0, 1.0),
                    (0.0, 0.0, 0.0, 1.0),
                    (0.0, 0.0, 0.0, 1.0),
                ],
                [
                    (1.0, 2.0, 3.0, 0.0),
                    (2.0, 2.0, 3.0, 0.0),
                    (10.0, 2.0, 3.0, 0.0),
                ],
                [
                    (0.0, 0.0, 0.0, 1.0),
                    (0.2, 0.1, 0.05, 1.0),
                    (0.0, 0.0, 0.0, 1.0),
                ],
            )

            summary = analyzer.summarize(
                analyzer.load_capture(capture_path),
                world_roi=(0.0, 0.0, 0.0, 5.0, 5.0, 5.0),
            )
            accepted = self.run_compatibility_analyzer(
                5,
                capture_path,
                "--world-roi",
                "0",
                "0",
                "0",
                "5",
                "5",
                "5",
                "--max-world-roi-environment-zero-count",
                "2",
                "--max-world-roi-combined-zero-count",
                "1",
            )
            rejected = self.run_compatibility_analyzer(
                5,
                capture_path,
                "--world-roi",
                "0",
                "0",
                "0",
                "5",
                "5",
                "5",
                "--max-world-roi-combined-zero-count",
                "0",
            )

        self.assertEqual(summary["world_roi_environment_zero_count"], 2)
        self.assertEqual(summary["world_roi_combined_zero_count"], 1)
        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)

    def test_world_roi_detects_mixed_zero_pixels_within_one_voxel_face(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "mixed-voxel-face.rfirr"
            voxel = 1.0 / analyzer.TERRAIN_VOXELS_PER_WORLD_UNIT
            self.write_capture_v5(
                capture_path,
                [
                    (0.0, 0.0, 0.0, 1.0),
                    (0.0, 0.0, 0.0, 1.0),
                    (0.2, 0.1, 0.05, 1.0),
                    (0.2, 0.1, 0.05, 1.0),
                    (0.0, 0.0, 0.0, 1.0),
                ],
                [
                    (voxel, 0.25 * voxel, 0.25 * voxel, 0.0),
                    (voxel, 0.35 * voxel, 0.35 * voxel, 0.0),
                    (voxel, 0.65 * voxel, 0.65 * voxel, 0.0),
                    (voxel, 0.75 * voxel, 0.75 * voxel, 0.0),
                    (2.0 * voxel, 0.25 * voxel, 0.25 * voxel, 0.0),
                ],
                [
                    (0.0, 0.0, 0.0, 1.0),
                    (0.0, 0.0, 0.0, 1.0),
                    (0.0, 0.0, 0.0, 1.0),
                    (0.0, 0.0, 0.0, 1.0),
                    (0.0, 0.0, 0.0, 1.0),
                ],
            )

            summary = analyzer.summarize(analyzer.load_capture(capture_path))
            accepted = self.run_compatibility_analyzer(
                5,
                capture_path,
                "--max-world-roi-mixed-environment-zero-voxel-face-count",
                "1",
                "--max-world-roi-mixed-combined-zero-voxel-face-count",
                "1",
            )
            rejected = self.run_compatibility_analyzer(
                5,
                capture_path,
                "--max-world-roi-mixed-environment-zero-voxel-face-count",
                "0",
                "--max-world-roi-mixed-combined-zero-voxel-face-count",
                "0",
            )

        self.assertEqual(summary["world_roi_quantized_voxel_face_count"], 2)
        self.assertEqual(
            summary["world_roi_mixed_environment_zero_voxel_face_count"], 1
        )
        self.assertEqual(
            summary["world_roi_mixed_combined_zero_voxel_face_count"], 1
        )
        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)

    def test_camera_position_detects_mixed_zero_pixels_within_one_receiver_voxel(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "mixed-receiver-voxel.rfirr"
            voxel = 1.0 / analyzer.TERRAIN_VOXELS_PER_WORLD_UNIT
            camera = (0.5 * voxel, 0.5 * voxel, -1.0)
            self.write_capture_v5(
                capture_path,
                [
                    (0.0, 0.0, 0.0, 1.0),
                    (0.0, 0.0, 0.0, 1.0),
                    (0.2, 0.1, 0.05, 1.0),
                    (0.2, 0.1, 0.05, 1.0),
                ],
                [
                    (0.25 * voxel, 0.25 * voxel, 0.0, 0.0),
                    (0.35 * voxel, 0.35 * voxel, 0.0, 0.0),
                    (0.65 * voxel, 0.65 * voxel, 0.0, 0.0),
                    (0.75 * voxel, 0.75 * voxel, 0.0, 0.0),
                ],
                [(0.0, 0.0, 0.0, 1.0)] * 4,
            )

            summary = analyzer.summarize(
                analyzer.load_capture(capture_path), camera_position=camera
            )
            rejected = self.run_compatibility_analyzer(
                5,
                capture_path,
                "--camera-position",
                *(str(value) for value in camera),
                "--max-world-roi-mixed-environment-zero-receiver-voxel-count",
                "0",
                "--max-world-roi-mixed-combined-zero-receiver-voxel-count",
                "0",
            )

        self.assertEqual(summary["world_roi_receiver_voxel_count"], 1)
        self.assertEqual(
            summary["world_roi_mixed_environment_zero_receiver_voxel_count"], 1
        )
        self.assertEqual(
            summary["world_roi_mixed_combined_zero_receiver_voxel_count"], 1
        )
        self.assertEqual(rejected.returncode, 1, rejected.stderr)

    def test_terrain_shadow_gate_uses_captured_marcher_voxel_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "subvoxel-direct-light.rfirr"
            voxel = 1.0 / analyzer.TERRAIN_VOXELS_PER_WORLD_UNIT
            world = [
                (0.20 * voxel, 0.20 * voxel, 0.0, 1.0),
                (0.35 * voxel, 0.35 * voxel, 0.0, 1.0),
                (0.65 * voxel, 0.65 * voxel, 0.0, 1.0),
                (0.80 * voxel, 0.80 * voxel, 0.0, 1.0),
            ]
            self.write_capture_v7(
                capture_path,
                [(0.0, 0.0, 0.0, 1.0)] * 4,
                world,
                [(0.25, 0.25, 0.25, 1.0)] * 4,
                [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, value)
                 for value in (0.25, 0.25, 0.75, 0.75)],
            )

            summary = analyzer.summarize(analyzer.load_capture(capture_path))
            accepted = self.run_compatibility_analyzer(
                7,
                capture_path,
                "--max-terrain-shadow-receiver-voxel-transmittance-range",
                "0.5",
            )
            rejected = self.run_compatibility_analyzer(
                7,
                capture_path,
                "--max-terrain-shadow-receiver-voxel-transmittance-range",
                "0.499",
            )

        self.assertTrue(summary["terrain_shadow_receiver_available"])
        self.assertTrue(summary["terrain_shadow_receiver_valid"])
        self.assertEqual(summary["terrain_shadow_receiver_voxel_count"], 1)
        self.assertAlmostEqual(
            summary["terrain_shadow_receiver_voxel_transmittance_range_max"],
            0.5,
        )
        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)
        self.assertIn(
            "terrain_shadow_receiver_voxel_transmittance_range_max: "
            "expected at most 0.499, got 0.5",
            json.loads(rejected.stdout)["validation_failures"],
        )

    def test_v8_leaf_shadow_gate_uses_captured_marcher_voxel_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "subvoxel-leaf-shadow.rfirr"
            voxel = 1.0 / analyzer.TERRAIN_VOXELS_PER_WORLD_UNIT
            receiver = [(0.5 * voxel, 0.5 * voxel, 0.5 * voxel, 1.0)] * 4
            leaf_values = (0.2, 0.2, 0.8, 0.8)
            self.write_capture_v8(
                capture_path,
                [(0.0, 0.0, 0.0, 1.0)] * 4,
                [(0.2 * voxel, 0.2 * voxel, 0.0, 1.0)] * 4,
                [(0.25, 0.25, 0.25, 1.0)] * 4,
                receiver,
                [(1.0, leaf, 1.0, leaf) for leaf in leaf_values],
            )

            capture = analyzer.load_capture(capture_path)
            summary = analyzer.summarize(capture)
            accepted = self.run_compatibility_analyzer(
                8,
                capture_path,
                "--max-leaf-shadow-receiver-voxel-transmittance-range",
                "0.601",
            )
            rejected = self.run_compatibility_analyzer(
                8,
                capture_path,
                "--max-leaf-shadow-receiver-voxel-transmittance-range",
                "0.599",
            )

        self.assertEqual(capture.version, 8)
        self.assertEqual(capture.plane_count, 5)
        self.assertTrue(summary["direct_sun_shadow_available"])
        self.assertTrue(summary["direct_sun_shadow_valid"])
        self.assertEqual(summary["leaf_shadow_receiver_voxel_count"], 1)
        self.assertAlmostEqual(
            summary["leaf_shadow_receiver_voxel_transmittance_range_max"], 0.6
        )
        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)
        self.assertIn(
            "leaf_shadow_receiver_voxel_transmittance_range_max: "
            "expected at most 0.599, got 0.6000000089406967",
            json.loads(rejected.stdout)["validation_failures"],
        )

    def test_world_roi_includes_gpu_positions_within_boundary_epsilon(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "world-roi-boundary.rfirr"
            self.write_capture_v3(
                capture_path,
                [(0.3, 0.1, 0.05, 1.0), (9.0, 9.0, 9.0, 1.0)],
                [
                    (0.5, 0.5, 0.49999988, 0.0),
                    (0.5, 0.5, 0.499, 0.0),
                ],
            )

            summary = analyzer.summarize(
                analyzer.load_capture(capture_path),
                world_roi=(0.0, 0.0, 0.5, 1.0, 1.0, 0.5),
            )

        self.assertEqual(summary["world_roi_terrain_hit_count"], 1)
        for actual, expected in zip(
            summary["world_roi_rgb_channel_mean"], [0.3, 0.1, 0.05]
        ):
            self.assertAlmostEqual(actual, expected)

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

            accepted = self.run_compatibility_analyzer(
                3,
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
            rejected = self.run_compatibility_analyzer(
                3,
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
            accepted = self.run_compatibility_analyzer(
                3, current_path, *common, "0.09"
            )
            rejected = self.run_compatibility_analyzer(
                3, current_path, *common, "0.11"
            )

        self.assertEqual(accepted.returncode, 0, accepted.stderr)
        gain = json.loads(accepted.stdout)["baseline_comparison"]
        self.assertTrue(gain["compatible"])
        self.assertAlmostEqual(gain["baseline_roi_luminance_mean"], 0.1)
        self.assertAlmostEqual(gain["current_roi_luminance_mean"], 0.2)
        self.assertAlmostEqual(gain["roi_luminance_gain"], 0.1)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)

    def test_luminance_gain_accepts_a_zero_energy_baseline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            baseline_path = Path(directory) / "dogleg-e0.rfirr"
            current_path = Path(directory) / "dogleg-e1.rfirr"
            world = [(1.0, 2.0, 3.0, 0.0)]
            self.write_capture_v6(
                baseline_path,
                [(0.0, 0.0, 0.0, 1.0)],
                world,
                [(0.0, 0.0, 0.0, 1.0)],
            )
            self.write_capture_v6(
                current_path,
                [(0.00004, 0.00004, 0.00004, 1.0)],
                world,
                [(0.0, 0.0, 0.0, 1.0)],
            )

            comparison = analyzer.compare_roi_baseline(
                analyzer.load_capture(current_path),
                analyzer.load_capture(baseline_path),
                (0.0, 0.0, 0.0, 2.0, 3.0, 4.0),
            )
            cli_result = self.run_compatibility_analyzer(
                6,
                current_path,
                "--baseline",
                str(baseline_path),
                "--world-roi",
                "0",
                "0",
                "0",
                "2",
                "3",
                "4",
                "--min-roi-luminance-gain",
                "0.000035",
            )

        self.assertTrue(comparison["compatible"])
        self.assertAlmostEqual(comparison["roi_luminance_gain"], 0.00004)
        self.assertIsNone(comparison["roi_channel_share_gain"])
        self.assertEqual(cli_result.returncode, 0, cli_result.stdout)

    def test_cli_gates_selected_channel_share_and_gain_over_baseline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            baseline_path = Path(directory) / "donor-s0.rfirr"
            current_path = Path(directory) / "donor-s1.rfirr"
            world = [(0.67, 0.50, 0.93749988, 0.0)]
            self.write_capture_v3(
                baseline_path,
                [(0.01, 0.08, 0.31, 1.0)],
                world,
                transport_stage=1,
                transport_iteration=0,
                publication_state=0,
            )
            self.write_capture_v3(
                current_path,
                [(0.08, 0.15, 0.44, 1.0)],
                world,
                transport_stage=2,
                transport_iteration=1,
            )
            roi = (
                "--world-roi",
                "0.53125",
                "0.4375",
                "0.9375",
                "0.8125",
                "0.59375",
                "0.9375",
                "--roi-channel",
                "red",
            )
            seed_accepted = self.run_compatibility_analyzer(
                3, baseline_path, *roi, "--max-roi-channel-share", "0.03"
            )
            gain_accepted = self.run_compatibility_analyzer(
                3,
                current_path,
                "--baseline",
                str(baseline_path),
                *roi,
                "--min-roi-channel-share-gain",
                "0.09",
            )
            gain_rejected = self.run_compatibility_analyzer(
                3,
                current_path,
                "--baseline",
                str(baseline_path),
                *roi,
                "--min-roi-channel-share-gain",
                "0.10",
            )

        self.assertEqual(seed_accepted.returncode, 0, seed_accepted.stderr)
        self.assertEqual(gain_accepted.returncode, 0, gain_accepted.stderr)
        comparison = json.loads(gain_accepted.stdout)["baseline_comparison"]
        self.assertGreater(comparison["selected_roi_channel_share_gain"], 0.09)
        self.assertEqual(gain_rejected.returncode, 1, gain_rejected.stderr)

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

            result = self.run_compatibility_analyzer(
                3,
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

            result = self.run_compatibility_analyzer(3, capture_path)

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

            result = self.run_compatibility_analyzer(3, capture_path)

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

            result = self.run_compatibility_analyzer(
                3,
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

            diagnostic = self.run_compatibility_analyzer(3, capture_path)
            correctness = self.run_compatibility_analyzer(
                3, capture_path, "--correctness"
            )

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

            result = self.run_compatibility_analyzer(3, capture_path)

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

            result = self.run_compatibility_analyzer(
                3, capture_path, "--require-zero-rgb"
            )

        self.assertEqual(result.returncode, 1, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["capture"]["rgb_channel_nonzero_count"], [0, 0, 1])
        self.assertIn(
            "terrain-hit RGB: expected exact zero, got 1 nonzero samples",
            report["validation_failures"],
        )

    def test_v3_nonnegative_gate_rejects_negative_terrain_hit_channel(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "negative-rgb.rfirr"
            self.write_capture_v3(
                capture_path,
                [(-0.000001, 0.2, 0.3, 1.0)],
                [(1.0, 2.0, 3.0, 0.0)],
            )

            result = self.run_compatibility_analyzer(
                3, capture_path, "--require-nonnegative-rgb"
            )

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

            luminance_only = self.run_compatibility_analyzer(
                2, capture_path, "--max-luminance", "0.00001"
            )
            exact_zero = self.run_compatibility_analyzer(
                2,
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
        self.assertIn(
            "terrain-hit RGB: expected exact zero, got 1 nonzero samples",
            report["validation_failures"],
        )

    def test_luminance_gates_report_the_failed_measurement(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            capture_path = Path(directory) / "luminance-gates.rfirr"
            self.write_capture(capture_path, [(0.1, 0.2, 0.3, 1.0)])

            maximum = self.run_analyzer(
                capture_path, "--max-luminance", "0.01"
            )
            minimum = self.run_analyzer(
                capture_path, "--min-luminance-p99", "1.0"
            )

        self.assertEqual(maximum.returncode, 1, maximum.stderr)
        self.assertEqual(minimum.returncode, 1, minimum.stderr)
        self.assertTrue(
            any(
                failure.startswith("luminance_max: expected at most 0.01")
                for failure in json.loads(maximum.stdout)["validation_failures"]
            )
        )
        self.assertTrue(
            any(
                failure.startswith("luminance_p99: expected at least 1")
                for failure in json.loads(minimum.stdout)["validation_failures"]
            )
        )


if __name__ == "__main__":
    unittest.main()
