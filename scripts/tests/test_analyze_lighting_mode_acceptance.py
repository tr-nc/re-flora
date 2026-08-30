from __future__ import annotations

import importlib.util
import struct
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
ANALYZER_PATH = REPO_ROOT / "scripts" / "analyze_lighting_mode_acceptance.py"
SPEC = importlib.util.spec_from_file_location("lighting_mode_acceptance", ANALYZER_PATH)
assert SPEC is not None and SPEC.loader is not None
analyzer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(analyzer)


def artifact(path: Path, manifest: str, payload: bytes = b"") -> None:
    encoded = manifest.encode("utf-8")
    path.write_bytes(b"RFLMA01\0" + struct.pack("<Q", len(encoded)) + encoded + payload)


def valid_artifact(path: Path) -> None:
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
                    "fnv1a64": analyzer.fnv1a64(raw),
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
    # Keep this writer independent of the Rust writer while exercising the public artifact seam.
    lines = [
        f'schema = "{analyzer.SCHEMA}"',
        f'calibration = "{analyzer.CALIBRATION}"',
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
            "layers": [
                {
                    "kind": kind,
                    "format": format_name,
                    "width": 1,
                    "height": 1,
                    "offset": 0,
                    "length": 4,
                    "fnv1a64": analyzer.fnv1a64(raw),
                }
                for kind, (format_name, _) in analyzer.LAYERS.items()
            ]
        }

        with self.assertRaisesRegex(ValueError, "overlap"):
            analyzer._phase_layers(phase, raw, [])


if __name__ == "__main__":
    unittest.main()
