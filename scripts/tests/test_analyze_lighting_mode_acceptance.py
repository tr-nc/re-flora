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
                if isinstance(value, str):
                    lines.append(f'{key} = "{value}"')
                elif type(value) is bool:
                    lines.append(f"{key} = {str(value).lower()}")
                else:
                    lines.append(f"{key} = {value}")
    artifact(path, "\n".join(lines) + "\n", bytes(payload))


def replace_layer_bytes(
    phases: list[dict[str, object]],
    payload: bytearray,
    kind: str,
    replacement: bytes,
    labels: str = "ABCD",
) -> None:
    for phase in phases:
        if phase["label"] not in labels:
            continue
        layers = phase["layers"]
        assert isinstance(layers, list)
        layer = next(layer for layer in layers if layer["kind"] == kind)
        offset = layer["offset"]
        length = layer["length"]
        assert isinstance(offset, int) and isinstance(length, int)
        assert len(replacement) == length
        payload[offset : offset + length] = replacement
        layer["fnv1a64"] = producer_fnv1a64(replacement)


class LightingModeAcceptanceAnalyzerTests(unittest.TestCase):
    def test_accepts_two_sided_raw_factorial_effects_with_exact_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.rflma"
            valid_artifact(path)

            result = analyzer.analyze(path)

        self.assertEqual(result["verdict"], "GREEN")
        self.assertEqual(result["schema"], "re-flora-lighting-mode-acceptance-v1")
        self.assertEqual(result["calibration"], "r13-e2-production-v1")
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

    def test_rejects_malformed_artifact_envelope(self) -> None:
        cases = (
            (b"", "shorter"),
            (b"NOTRFLMA" + struct.pack("<Q", 0), "magic"),
            (b"RFLMA01\0" + struct.pack("<Q", 99), "manifest length"),
            (b"RFLMA01\0" + struct.pack("<Q", 1) + b"\xff", "invalid artifact manifest"),
        )
        for raw, reason in cases:
            with self.subTest(reason=reason), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "capture.rflma"
                path.write_bytes(raw)
                with self.assertRaisesRegex(ValueError, reason):
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

    def test_rejects_phase_contract_drift_and_missing_identity(self) -> None:
        cases = ("label", "terrain_mode", "raster_mode", "missing_identity", "identity_drift")
        for case in cases:
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "capture.rflma"

                def corrupt_phase(
                    phases: list[dict[str, object]], _: bytearray
                ) -> None:
                    if case == "missing_identity":
                        del phases[0]["camera_pose_bits"]
                    elif case == "identity_drift":
                        phases[1]["camera_pose_bits"] = [6, 5, 4, 3, 2, 1]
                    else:
                        phases[1][case] = "unexpected"

                valid_artifact(path, corrupt_phase)
                expected = "missing" if case == "missing_identity" else "drifted" if case == "identity_drift" else "mismatch"
                with self.assertRaisesRegex(ValueError, expected):
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

    def test_rejects_equal_but_illegal_production_identity(self) -> None:
        invalid_cases: tuple[tuple[str, object], ...] = (
            ("fixture", "caller-fixture"),
            ("visual_time_bits", 1),
            ("sampling_serial", 7),
            ("binary_identity", "fnv1a64:not-hex"),
            ("camera_pose_bits", [1, 2, 3, 4, 5]),
            ("camera_pose_bits", [1, 2, 3, 4, 5, -1]),
            ("screen_extent", [0, 2]),
            ("extent_generation", -1),
            ("visible_terrain_revision", -1),
            ("ddgi_field_serial", -1),
        )
        for field, invalid_value in invalid_cases:
            with self.subTest(field=field), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "capture.rflma"

                def corrupt_identity(
                    phases: list[dict[str, object]], _: bytearray
                ) -> None:
                    for phase in phases:
                        phase[field] = invalid_value

                valid_artifact(path, corrupt_identity)

                with self.assertRaisesRegex(ValueError, field):
                    analyzer.analyze(path)

    def test_rejects_illegal_ddgi_field_source_relationships(self) -> None:
        cases: tuple[tuple[str, object], ...] = (
            ("visible_terrain_revision", 0),
            ("ddgi_field_serial", 0),
            ("ddgi_radiance_revision", 0),
            ("ddgi_spacing_voxels", 0),
            ("ddgi_update_epoch", 0),
            ("ddgi_source_field_serial", 0),
            ("ddgi_source_radiance_revision", 0),
            ("ddgi_geometry_revision", 3),
            ("ddgi_source_geometry_revision", 3),
            ("ddgi_source_radiance_revision", 5),
            ("ddgi_source_field_serial", 3),
            ("ddgi_source_update_epoch", 8),
        )
        for field, invalid_value in cases:
            with self.subTest(field=field), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "capture.rflma"

                def corrupt_ddgi(
                    phases: list[dict[str, object]], _: bytearray
                ) -> None:
                    for phase in phases:
                        phase[field] = invalid_value

                valid_artifact(path, corrupt_ddgi)

                with self.assertRaisesRegex(ValueError, "DDGI identity"):
                    analyzer.analyze(path)

    def test_rejects_nonfinite_depth_and_empty_masks(self) -> None:
        cases = ("nonfinite-depth", "empty-terrain", "empty-raster")
        for case in cases:
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "capture.rflma"

                def corrupt_mask(
                    phases: list[dict[str, object]], payload: bytearray
                ) -> None:
                    if case == "nonfinite-depth":
                        replacement = struct.pack("<20f", float("nan"), *([0.5] * 19))
                        replace_layer_bytes(phases, payload, "terrain_depth", replacement)
                    elif case == "empty-terrain":
                        replace_layer_bytes(
                            phases, payload, "terrain_depth", struct.pack("<20f", *([1.0] * 20))
                        )
                    else:
                        empty = bytes(80)
                        replace_layer_bytes(phases, payload, "raster_rgba", empty)

                valid_artifact(path, corrupt_mask)

                expected = "non-finite" if case == "nonfinite-depth" else "mask is empty"
                with self.assertRaisesRegex(ValueError, expected):
                    analyzer.analyze(path)

    def test_rejects_depth_or_exact_alpha_mask_drift(self) -> None:
        for case in ("depth", "alpha"):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "capture.rflma"

                def drift_mask_source(
                    phases: list[dict[str, object]], payload: bytearray
                ) -> None:
                    if case == "depth":
                        replace_layer_bytes(
                            phases,
                            payload,
                            "terrain_depth",
                            struct.pack("<20f", *([0.25] * 20)),
                            labels="B",
                        )
                    else:
                        replace_layer_bytes(
                            phases,
                            payload,
                            "raster_rgba",
                            bytes([0, 0, 0, 254] * 20),
                            labels="B",
                        )

                valid_artifact(path, drift_mask_source)
                expected = "depth identity" if case == "depth" else "alpha mask"
                with self.assertRaisesRegex(ValueError, expected):
                    analyzer.analyze(path)

    def test_changed_count_rejects_layer_or_mask_length_mismatch(self) -> None:
        for second, mask in ((memoryview(bytes(4)), [True, True]), (memoryview(bytes(8)), [True])):
            with self.subTest(second=len(second), mask=len(mask)):
                with self.assertRaisesRegex(ValueError, "length"):
                    analyzer._changed_count(memoryview(bytes(8)), second, 4, mask)

    def test_rejects_one_sided_effect_before_non_target_orthogonality(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.rflma"

            def remove_second_terrain_effect(
                phases: list[dict[str, object]], payload: bytearray
            ) -> None:
                replacement = bytes([1, 0, 0, 0] * 20)
                replace_layer_bytes(phases, payload, "terrain_rgbe", replacement, labels="D")

            valid_artifact(path, remove_second_terrain_effect)

            with self.assertRaisesRegex(ValueError, "terrain D/C raw effect"):
                analyzer.analyze(path)

    def test_rejects_non_target_orthogonality_with_two_sided_effects(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.rflma"

            def drift_non_target_pair(
                phases: list[dict[str, object]], payload: bytearray
            ) -> None:
                replace_layer_bytes(
                    phases, payload, "terrain_rgbe", bytes([2, 0, 0, 0] * 20), labels="D"
                )
                replace_layer_bytes(
                    phases, payload, "terrain_rgbe", bytes([3, 0, 0, 0] * 20), labels="C"
                )

            valid_artifact(path, drift_non_target_pair)

            with self.assertRaisesRegex(ValueError, "terrain A/D orthogonality"):
                analyzer.analyze(path)

    def test_rejects_each_other_non_target_orthogonality_pair(self) -> None:
        cases = (
            ("terrain_rgbe", "C", bytes([2, 0, 0, 0] * 20), "terrain B/C"),
            ("raster_rgba", "B", bytes([2, 0, 0, 255] * 20), "raster A/B"),
            ("raster_rgba", "D", bytes([2, 0, 0, 255] * 20), "raster C/D"),
        )
        for kind, label, replacement, reason in cases:
            with self.subTest(reason=reason), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "capture.rflma"

                def drift_pair(
                    phases: list[dict[str, object]], payload: bytearray
                ) -> None:
                    replace_layer_bytes(phases, payload, kind, replacement, labels=label)

                valid_artifact(path, drift_pair)
                with self.assertRaisesRegex(ValueError, reason):
                    analyzer.analyze(path)

    def test_rejects_malformed_layer_contract_and_payload_coverage(self) -> None:
        cases = ("format", "length", "hash", "gap", "suffix")
        for case in cases:
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "capture.rflma"

                def corrupt_layer(
                    phases: list[dict[str, object]], payload: bytearray
                ) -> None:
                    layers = phases[0]["layers"]
                    assert isinstance(layers, list)
                    first = layers[0]
                    assert isinstance(first, dict)
                    if case == "format":
                        first["format"] = "R8G8B8A8_UNORM"
                    elif case == "length":
                        first["length"] = 79
                    elif case == "hash":
                        first["fnv1a64"] = "0000000000000000"
                    elif case == "suffix":
                        payload.append(0)
                    else:
                        insertion = first["offset"] + first["length"]
                        assert isinstance(insertion, int)
                        payload[insertion:insertion] = b"\0"
                        for phase in phases:
                            phase_layers = phase["layers"]
                            assert isinstance(phase_layers, list)
                            for layer in phase_layers:
                                offset = layer["offset"]
                                assert isinstance(offset, int)
                                if offset >= insertion and layer is not first:
                                    layer["offset"] = offset + 1

                valid_artifact(path, corrupt_layer)

                expected = {
                    "format": "format mismatch",
                    "length": "byte length mismatch",
                    "hash": "hash mismatch",
                    "gap": "unclaimed gap",
                    "suffix": "unclaimed suffix",
                }[case]
                with self.assertRaisesRegex(ValueError, expected):
                    analyzer.analyze(path)

    def test_rejects_boolean_layer_descriptor_integers(self) -> None:
        for field in ("width", "height", "offset", "length"):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "capture.rflma"

                def corrupt_descriptor(
                    phases: list[dict[str, object]], _: bytearray
                ) -> None:
                    layers = phases[0]["layers"]
                    assert isinstance(layers, list)
                    layer = layers[0]
                    assert isinstance(layer, dict)
                    layer[field] = True

                valid_artifact(path, corrupt_descriptor)
                with self.assertRaisesRegex(ValueError, "invalid dimensions or range"):
                    analyzer.analyze(path)


if __name__ == "__main__":
    unittest.main()
