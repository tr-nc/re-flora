#!/usr/bin/env python3
"""Fail-closed analyzer for the R13 E2 production lighting-mode artifact."""

from __future__ import annotations

import argparse
import json
import math
import re
import struct
import sys
import tomllib
from pathlib import Path
from typing import Any


MAGIC = b"RFLMA01\0"
SCHEMA = "re-flora-lighting-mode-acceptance-v1"
CALIBRATION = "r13-e2-production-v1"
PHASES = (
    ("A", "ddgi", "ddgi"),
    ("B", "path-reference", "ddgi"),
    ("C", "path-reference", "legacy"),
    ("D", "ddgi", "legacy"),
)
LAYERS = {
    "terrain_rgbe": ("R32_UINT", 4),
    "terrain_depth": ("R32_SFLOAT", 4),
    "raster_rgba": ("R8G8B8A8_UNORM", 4),
}
MIN_CHANGED_PIXELS = 16
MIN_CHANGED_RATIO = 1.0e-6
FIXTURE = "foliage-shadow-r13-e2-v1"
FROZEN_VISUAL_TIME_BITS = 0
FROZEN_SAMPLING_SERIAL = 0x52461302
IDENTITY_FIELDS = (
    "binary_identity",
    "fixture",
    "camera_pose_bits",
    "render_extent",
    "screen_extent",
    "extent_generation",
    "visible_terrain_revision",
    "ddgi_field_serial",
    "ddgi_geometry_revision",
    "ddgi_radiance_revision",
    "ddgi_spacing_voxels",
    "ddgi_update_epoch",
    "ddgi_source_field_serial",
    "ddgi_source_geometry_revision",
    "ddgi_source_radiance_revision",
    "ddgi_source_update_epoch",
    "authored_lighting_revision",
    "local_lighting_revision",
    "visual_time_bits",
    "sampling_serial",
)


def fnv1a64(data: bytes) -> str:
    value = 0xCBF29CE484222325
    for byte in data:
        value ^= byte
        value = (value * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{value:016x}"


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def _require_u32(value: object, field: str) -> int:
    _require(type(value) is int and 0 <= value <= 0xFFFFFFFF, f"{field} must be a u32")
    return value


def _require_u64(value: object, field: str) -> int:
    _require(
        type(value) is int and 0 <= value <= 0xFFFFFFFFFFFFFFFF,
        f"{field} must be a u64",
    )
    return value


def _require_extent(value: object, field: str) -> tuple[int, int]:
    _require(
        isinstance(value, list)
        and len(value) == 2
        and all(type(dimension) is int and 0 < dimension <= 0xFFFFFFFF for dimension in value),
        f"phase {field} must contain exactly two positive u32 integers",
    )
    return value[0], value[1]


def _require_identity(phase: dict[str, Any], label: str) -> None:
    _require(phase["fixture"] == FIXTURE, f"phase {label} fixture is not production fixture")
    _require(
        isinstance(phase["binary_identity"], str)
        and re.fullmatch(r"fnv1a64:[0-9a-f]{16}", phase["binary_identity"]) is not None,
        f"phase {label} binary_identity is malformed",
    )
    camera = phase["camera_pose_bits"]
    _require(
        isinstance(camera, list)
        and len(camera) == 6
        and all(type(component) is int and 0 <= component <= 0xFFFFFFFF for component in camera),
        f"phase {label} camera_pose_bits must contain exactly six u32 integers",
    )
    _require_extent(phase["render_extent"], "render_extent")
    _require_extent(phase["screen_extent"], "screen_extent")
    for field in (
        "extent_generation",
        "ddgi_field_serial",
        "ddgi_source_field_serial",
        "authored_lighting_revision",
        "local_lighting_revision",
    ):
        _require_u64(phase[field], field)
    for field in (
        "visible_terrain_revision",
        "ddgi_geometry_revision",
        "ddgi_radiance_revision",
        "ddgi_spacing_voxels",
        "ddgi_update_epoch",
        "ddgi_source_geometry_revision",
        "ddgi_source_radiance_revision",
        "ddgi_source_update_epoch",
        "visual_time_bits",
        "sampling_serial",
    ):
        _require_u32(phase[field], field)
    _require(phase["visual_time_bits"] == FROZEN_VISUAL_TIME_BITS, f"phase {label} visual_time_bits is not frozen")
    _require(phase["sampling_serial"] == FROZEN_SAMPLING_SERIAL, f"phase {label} sampling_serial is not frozen")
    _require(
        phase["visible_terrain_revision"] > 0
        and phase["ddgi_field_serial"] > 0
        and phase["ddgi_radiance_revision"] > 0
        and phase["ddgi_spacing_voxels"] > 0
        and phase["ddgi_update_epoch"] > 0
        and phase["ddgi_source_field_serial"] > 0
        and phase["ddgi_source_radiance_revision"] > 0
        and phase["visible_terrain_revision"] == phase["ddgi_geometry_revision"]
        and phase["ddgi_source_geometry_revision"] == phase["ddgi_geometry_revision"]
        and phase["ddgi_source_radiance_revision"] == phase["ddgi_radiance_revision"]
        and phase["ddgi_source_field_serial"] != phase["ddgi_field_serial"]
        and phase["ddgi_source_update_epoch"] + 1 == phase["ddgi_update_epoch"],
        f"phase {label} DDGI identity has an illegal field/source relationship",
    )


def _load(path: Path) -> tuple[dict[str, Any], bytes]:
    raw = path.read_bytes()
    _require(len(raw) >= 16, "artifact is shorter than its fixed header")
    _require(raw[:8] == MAGIC, "artifact magic is not RFLMA01")
    manifest_length = struct.unpack_from("<Q", raw, 8)[0]
    manifest_end = 16 + manifest_length
    _require(manifest_end <= len(raw), "artifact manifest length exceeds file")
    try:
        manifest = tomllib.loads(raw[16:manifest_end].decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise ValueError(f"invalid artifact manifest: {error}") from error
    return manifest, raw[manifest_end:]


def _phase_layers(
    phase: dict[str, Any], payload: bytes, occupied: list[tuple[int, int]]
) -> dict[str, memoryview]:
    descriptors = phase.get("layers")
    _require(isinstance(descriptors, list), "phase has no raw production layers")
    _require(
        all(isinstance(descriptor, dict) for descriptor in descriptors),
        "phase raw production layer descriptor is malformed",
    )
    kinds = [descriptor.get("kind") for descriptor in descriptors]
    _require(len(kinds) == len(set(kinds)), "phase has a duplicate raw production layer kind")
    by_kind = {descriptor.get("kind"): descriptor for descriptor in descriptors}
    _require(set(by_kind) == set(LAYERS), "phase raw production layer set is incomplete")
    render_width, render_height = _require_extent(phase.get("render_extent"), "render_extent")
    result: dict[str, memoryview] = {}
    for kind, (expected_format, bytes_per_pixel) in LAYERS.items():
        descriptor = by_kind[kind]
        _require(descriptor.get("format") == expected_format, f"{kind} format mismatch")
        width = descriptor.get("width")
        height = descriptor.get("height")
        offset = descriptor.get("offset")
        length = descriptor.get("length")
        _require(
            all(isinstance(value, int) and value >= 0 for value in (width, height, offset, length)),
            f"{kind} descriptor contains invalid dimensions or range",
        )
        _require(width > 0 and height > 0, f"{kind} extent is empty")
        _require(
            (width, height) == (render_width, render_height),
            f"{kind} extent does not match phase render_extent",
        )
        _require(length == width * height * bytes_per_pixel, f"{kind} byte length mismatch")
        _require(offset + length <= len(payload), f"{kind} payload range exceeds artifact")
        span = (offset, offset + length)
        _require(
            all(span[1] <= other[0] or span[0] >= other[1] for other in occupied),
            f"{kind} payload range overlaps another layer",
        )
        occupied.append(span)
        layer = memoryview(payload)[offset : offset + length]
        _require(fnv1a64(layer) == descriptor.get("fnv1a64"), f"{kind} hash mismatch")
        result[kind] = layer
    return result


def _changed_count(first: memoryview, second: memoryview, stride: int, mask: list[bool]) -> int:
    _require(stride > 0, "changed-count stride must be positive")
    _require(len(first) == len(second), "changed-count layer lengths differ")
    _require(len(first) == len(mask) * stride, "changed-count layer and mask lengths differ")
    return sum(
        first[index * stride : (index + 1) * stride]
        != second[index * stride : (index + 1) * stride]
        for index, selected in enumerate(mask)
        if selected
    )


def _require_effect(label: str, changed: int, population: int) -> None:
    _require(population > 0, f"{label} mask is empty")
    ratio = changed / population
    _require(
        changed >= MIN_CHANGED_PIXELS and ratio >= MIN_CHANGED_RATIO,
        f"{label} raw effect is below committed calibration: changed={changed} ratio={ratio:.9f}",
    )


def analyze(path: Path) -> dict[str, Any]:
    manifest, payload = _load(path)
    _require(manifest.get("schema") == SCHEMA, "unknown artifact schema")
    _require(manifest.get("calibration") == CALIBRATION, "unknown artifact calibration")
    phases = manifest.get("phases")
    _require(isinstance(phases, list) and len(phases) == 4, "artifact must contain four phases")
    _require(manifest.get("phase_count") == 4, "artifact phase_count must be four")

    baseline = phases[0]
    decoded_layers: list[dict[str, memoryview]] = []
    occupied: list[tuple[int, int]] = []
    for phase, (label, terrain_mode, raster_mode) in zip(phases, PHASES, strict=True):
        _require(phase.get("label") == label, f"phase order mismatch at {label}")
        _require(phase.get("terrain_mode") == terrain_mode, f"phase {label} terrain mode mismatch")
        _require(phase.get("raster_mode") == raster_mode, f"phase {label} raster mode mismatch")
        for field in IDENTITY_FIELDS:
            _require(field in phase, f"phase {label} identity is missing {field}")
            _require(phase[field] == baseline[field], f"phase {label} identity drifted at {field}")
        _require_identity(phase, label)
        decoded_layers.append(_phase_layers(phase, payload, occupied))

    ordered_ranges = sorted(occupied)
    _require(ordered_ranges[0][0] == 0, "raw payload has an unclaimed prefix")
    _require(
        all(first[1] == second[0] for first, second in zip(ordered_ranges, ordered_ranges[1:])),
        "raw payload has an unclaimed gap",
    )
    _require(ordered_ranges[-1][1] == len(payload), "raw payload has an unclaimed suffix")

    a, b, c, d = decoded_layers
    for phase in decoded_layers[1:]:
        _require(
            bytes(phase["terrain_depth"]) == bytes(a["terrain_depth"]),
            "terrain depth identity changed across phases",
        )
    depth = struct.iter_unpack("<f", a["terrain_depth"])
    terrain_mask = []
    for (value,) in depth:
        _require(math.isfinite(value), "terrain depth contains a non-finite value")
        terrain_mask.append(0.0 < value < 1.0)

    alpha_mask = [a["raster_rgba"][index] > 0 for index in range(3, len(a["raster_rgba"]), 4)]
    for phase in decoded_layers[1:]:
        alpha = [phase["raster_rgba"][index] for index in range(3, len(phase["raster_rgba"]), 4)]
        _require(alpha == [a["raster_rgba"][index] for index in range(3, len(a["raster_rgba"]), 4)], "raster alpha mask changed across phases")

    terrain_ab = _changed_count(a["terrain_rgbe"], b["terrain_rgbe"], 4, terrain_mask)
    terrain_dc = _changed_count(d["terrain_rgbe"], c["terrain_rgbe"], 4, terrain_mask)
    raster_ad = _changed_count(a["raster_rgba"], d["raster_rgba"], 4, alpha_mask)
    raster_bc = _changed_count(b["raster_rgba"], c["raster_rgba"], 4, alpha_mask)
    _require_effect("terrain A/B", terrain_ab, sum(terrain_mask))
    _require_effect("terrain D/C", terrain_dc, sum(terrain_mask))
    _require_effect("raster A/D", raster_ad, sum(alpha_mask))
    _require_effect("raster B/C", raster_bc, sum(alpha_mask))

    _require(bytes(a["terrain_rgbe"]) == bytes(d["terrain_rgbe"]), "terrain A/D orthogonality failed")
    _require(bytes(b["terrain_rgbe"]) == bytes(c["terrain_rgbe"]), "terrain B/C orthogonality failed")
    _require(bytes(a["raster_rgba"]) == bytes(b["raster_rgba"]), "raster A/B orthogonality failed")
    _require(bytes(c["raster_rgba"]) == bytes(d["raster_rgba"]), "raster C/D orthogonality failed")

    return {
        "verdict": "GREEN",
        "artifact": str(path),
        "calibration": CALIBRATION,
        "terrain_pixels": sum(terrain_mask),
        "raster_pixels": sum(alpha_mask),
        "terrain_changed_ab": terrain_ab,
        "terrain_changed_dc": terrain_dc,
        "raster_changed_ad": raster_ad,
        "raster_changed_bc": raster_bc,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("artifact", type=Path)
    args = parser.parse_args()
    try:
        result = analyze(args.artifact)
    except (OSError, ValueError) as error:
        print(f"[LIGHTING_MODE_ACCEPTANCE] verdict=RED reason={error}", file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
