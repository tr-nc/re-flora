from __future__ import annotations

import importlib.util
import struct
import tempfile
import unittest
from pathlib import Path
from typing import Callable


REPO_ROOT = Path(__file__).resolve().parents[2]
ANALYZER_PATH = REPO_ROOT / "scripts" / "analyze_lighting_mode_acceptance.py"
SPEC = importlib.util.spec_from_file_location("lighting_mode_acceptance", ANALYZER_PATH)
assert SPEC is not None and SPEC.loader is not None
analyzer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(analyzer)

FROZEN_SCHEMA = "re-flora-lighting-mode-acceptance-v1"
FROZEN_CALIBRATION = "r13-e2-production-v1"


def producer_fnv1a64(data: bytes) -> str:
    value = 0xCBF29CE484222325
    for byte in data:
        value ^= byte
        value = (value * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{value:016x}"


def artifact(path: Path, manifest: str, payload: bytes = b"") -> None:
    encoded = manifest.encode("utf-8")
    path.write_bytes(b"RFLMA01\0" + struct.pack("<Q", len(encoded)) + encoded + payload)


def valid_artifact(
    path: Path,
    mutate: Callable[[list[dict[str, object]], bytearray], None] | None = None,
) -> None:
    width, height = 20, 1
    depth = struct.pack("<20f", *([0.5] * 20))
    terrain = {"A": bytes(80), "B": bytes([1, 0, 0, 0] * 20)}
    terrain["C"] = terrain["B"]
    terrain["D"] = terrain["A"]
    raster = {"A": bytes([0, 0, 0, 255] * 20)}
    raster["B"] = raster["A"]
    raster["C"] = bytes([1, 0, 0, 255] * 20)
    raster["D"] = raster["C"]
    identity = {
        "binary_identity": "fnv1a64:0123456789abcdef",
        "fixture": "foliage-shadow-r13-e2-v1",
        "camera_pose_bits": [1, 2, 3, 4, 5, 6],
        "render_extent": [width, height],
        "screen_extent": [40, 2],
        "extent_generation": 1,
        "visible_terrain_revision": 2,
        "ddgi_field_serial": 3,
        "ddgi_geometry_revision": 2,
        "ddgi_radiance_revision": 4,
        "ddgi_spacing_voxels": 32,
        "ddgi_update_epoch": 8,
        "ddgi_source_field_serial": 2,
        "ddgi_source_geometry_revision": 2,
        "ddgi_source_radiance_revision": 4,
        "ddgi_source_update_epoch": 7,
        "authored_lighting_revision": 4,
        "local_lighting_revision": 5,
        "visual_time_bits": 0,
        "sampling_serial": 0x52461302,
    }
    payload = bytearray()
    phase_records = []
    modes = {
        "A": ("ddgi", "ddgi"),
        "B": ("path-reference", "ddgi"),
        "C": ("path-reference", "legacy"),
        "D": ("ddgi", "legacy"),
    }
    for label in "ABCD":
        layers = []
        for kind, format_name, raw in (
            ("terrain_rgbe", "R32_UINT", terrain[label]),
            ("terrain_depth", "R32_SFLOAT", depth),
            ("raster_rgba", "R8G8B8A8_UNORM", raster[label]),
        ):
            offset = len(payload)
            payload.extend(raw)
            layers.append(
                {
                    "kind": kind,
                    "format": format_name,
                    "width": width,
                    "height": height,
                    "offset": offset,
                    "length": len(raw),
                    "fnv1a64": producer_fnv1a64(raw),
                }
            )
        terrain_mode, raster_mode = modes[label]
        phase_records.append(
            {
                "label": label,
                "terrain_mode": terrain_mode,
                "raster_mode": raster_mode,
                **identity,
                "layers": layers,
            }
        )
    if mutate is not None:
        mutate(phase_records, payload)
    # Keep this writer independent of the Rust writer while exercising the public artifact seam.
    lines = [
        f'schema = "{FROZEN_SCHEMA}"',
        f'calibration = "{FROZEN_CALIBRATION}"',
        "phase_count = 4",
    ]
    for phase in phase_records:
        lines.append("[[phases]]")
        for key, value in phase.items():
            if key == "layers":
                continue
            if isinstance(value, str):
                lines.append(f'{key} = "{value}"')
            elif isinstance(value, list):
                lines.append(f"{key} = {value}")
            else:
                lines.append(f"{key} = {value}")
        for layer in phase["layers"]:
            lines.append("[[phases.layers]]")
            for key, value in layer.items():
                lines.append(f'{key} = "{value}"' if isinstance(value, str) else f"{key} = {value}")
    artifact(path, "\n".join(lines) + "\n", bytes(payload))


class LightingModeAcceptanceAnalyzerTests(unittest.TestCase):
    def test_accepts_two_sided_raw_factorial_effects_with_exact_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.rflma"
            valid_artifact(path)

            result = analyzer.analyze(path)

        self.assertEqual(result["verdict"], "GREEN")
        self.assertEqual(result["terrain_changed_ab"], 20)
        self.assertEqual(result["raster_changed_ad"], 20)

    def test_rejects_unknown_calibration_before_reading_payload(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.rflma"
            artifact(
                path,
                'schema = "re-flora-lighting-mode-acceptance-v1"\n'
                'calibration = "caller-controlled"\n',
            )

            with self.assertRaisesRegex(ValueError, "calibration"):
                analyzer.analyze(path)

    def test_rejects_target_flags_without_all_raw_production_layers(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.rflma"
            artifact(
                path,
                'schema = "re-flora-lighting-mode-acceptance-v1"\n'
                'calibration = "r13-e2-production-v1"\n'
                'phase_count = 4\n',
            )

            with self.assertRaisesRegex(ValueError, "phase"):
                analyzer.analyze(path)

    def test_rejects_overlapping_raw_layer_ranges(self) -> None:
        raw = b"\0\0\0\0"
        phase = {
            "render_extent": [1, 1],
            "layers": [
                {
                    "kind": kind,
                    "format": format_name,
                    "width": 1,
                    "height": 1,
                    "offset": 0,
                    "length": 4,
                    "fnv1a64": producer_fnv1a64(raw),
                }
                for kind, (format_name, _) in analyzer.LAYERS.items()
            ]
        }

        with self.assertRaisesRegex(ValueError, "overlap"):
            analyzer._phase_layers(phase, raw, [])

    def test_rejects_duplicate_raw_layer_kind(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.rflma"

            def duplicate_kind(phases: list[dict[str, object]], _: bytearray) -> None:
                layers = phases[0]["layers"]
                assert isinstance(layers, list)
                layers.append(dict(layers[0]))

            valid_artifact(path, duplicate_kind)

            with self.assertRaisesRegex(ValueError, "duplicate"):
                analyzer.analyze(path)

    def test_rejects_layer_extent_that_disagrees_with_render_extent(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.rflma"

            def reshape_layers(phases: list[dict[str, object]], _: bytearray) -> None:
                for phase in phases:
                    layers = phase["layers"]
                    assert isinstance(layers, list)
                    for layer in layers:
                        assert isinstance(layer, dict)
                        layer["width"] = 10
                        layer["height"] = 2

            valid_artifact(path, reshape_layers)

            with self.assertRaisesRegex(ValueError, "render_extent"):
                analyzer.analyze(path)

    def test_rejects_different_extents_between_layers(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.rflma"

            def reshape_depth(phases: list[dict[str, object]], _: bytearray) -> None:
                for phase in phases:
                    layers = phase["layers"]
                    assert isinstance(layers, list)
                    depth = layers[1]
                    assert isinstance(depth, dict)
                    depth["width"] = 10
                    depth["height"] = 2

            valid_artifact(path, reshape_depth)

            with self.assertRaisesRegex(ValueError, "render_extent"):
                analyzer.analyze(path)

    def test_rejects_non_positive_or_non_integer_render_extent(self) -> None:
        invalid_extents: tuple[object, ...] = (
            [0, 1],
            [-1, 1],
            [20, 0],
            [20],
            [20, 1, 1],
            [20.0, 1],
            20,
        )
        for invalid_extent in invalid_extents:
            with self.subTest(render_extent=invalid_extent):
                with tempfile.TemporaryDirectory() as directory:
                    path = Path(directory) / "capture.rflma"

                    def corrupt_extent(
                        phases: list[dict[str, object]], _: bytearray
                    ) -> None:
                        for phase in phases:
                            phase["render_extent"] = invalid_extent

                    valid_artifact(path, corrupt_extent)

                    with self.assertRaisesRegex(ValueError, "render_extent"):
                        analyzer.analyze(path)


if __name__ == "__main__":
    unittest.main()
