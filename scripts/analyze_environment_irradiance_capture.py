#!/usr/bin/env python3
"""Inspect deterministic pre-albedo environment-irradiance captures."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import struct
from dataclasses import dataclass
from pathlib import Path


MAGIC = b"RFIRR001"
HEADER_PREFIX = struct.Struct("<8sI")
HEADER_V1 = struct.Struct("<8s6I")
HEADER_V2 = struct.Struct("<8s7I")
PIXEL = struct.Struct("<4f")

DEBUG_VIEW_LABELS = {
    0: "final",
    1: "moment-visibility",
    2: "exact-visibility",
    3: "visibility-error",
    4: "exact-irradiance",
    5: "irradiance-error",
    6: "weight-sum",
    7: "dominant-probe",
    8: "probe-state",
    9: "relocation",
    10: "irradiance-atlas",
    11: "visibility-atlas",
}


@dataclass(frozen=True)
class Capture:
    path: Path
    width: int
    height: int
    backend: int
    spacing_voxels: int
    debug_view: int
    payload: bytes

    @property
    def sample_count(self) -> int:
        return self.width * self.height


def load_capture(path: Path) -> Capture:
    data = path.read_bytes()
    if len(data) < HEADER_PREFIX.size:
        raise ValueError(f"{path}: truncated header")
    magic, version = HEADER_PREFIX.unpack_from(data)
    if magic != MAGIC:
        raise ValueError(f"{path}: invalid magic {magic!r}")
    if version == 1:
        if len(data) < HEADER_V1.size:
            raise ValueError(f"{path}: truncated v1 header")
        _, _, width, height, channels, backend, spacing = HEADER_V1.unpack_from(data)
        debug_view = 0
        header_size = HEADER_V1.size
    elif version == 2:
        if len(data) < HEADER_V2.size:
            raise ValueError(f"{path}: truncated v2 header")
        _, _, width, height, channels, backend, spacing, debug_view = HEADER_V2.unpack_from(data)
        header_size = HEADER_V2.size
    else:
        raise ValueError(f"{path}: unsupported version {version}")
    if channels != 4:
        raise ValueError(f"{path}: expected four float channels, got {channels}")
    payload = data[header_size:]
    expected = width * height * PIXEL.size
    if len(payload) != expected:
        raise ValueError(f"{path}: payload is {len(payload)} bytes, expected {expected}")
    return Capture(path, width, height, backend, spacing, debug_view, payload)


def percentile(sorted_values: list[float], fraction: float) -> float:
    if not sorted_values:
        return 0.0
    index = math.ceil(fraction * len(sorted_values)) - 1
    return sorted_values[max(0, min(index, len(sorted_values) - 1))]


def summarize(capture: Capture) -> dict[str, object]:
    luminances: list[float] = []
    finite = True
    for red, green, blue, hit in PIXEL.iter_unpack(capture.payload):
        finite = finite and all(math.isfinite(value) for value in (red, green, blue, hit))
        if hit > 0.5:
            luminances.append(0.2126 * red + 0.7152 * green + 0.0722 * blue)
    luminances.sort()
    return {
        "path": str(capture.path),
        "width": capture.width,
        "height": capture.height,
        "backend": capture.backend,
        "spacing_voxels": capture.spacing_voxels,
        "debug_view": DEBUG_VIEW_LABELS.get(capture.debug_view, capture.debug_view),
        "sample_count": capture.sample_count,
        "terrain_hit_count": len(luminances),
        "finite": finite,
        "luminance_mean": sum(luminances) / len(luminances) if luminances else 0.0,
        "luminance_p99": percentile(luminances, 0.99),
        "luminance_max": luminances[-1] if luminances else 0.0,
        "payload_sha256": hashlib.sha256(capture.payload).hexdigest(),
    }


def compare(first: Capture, second: Capture) -> dict[str, object]:
    compatible = (
        first.width,
        first.height,
        first.backend,
        first.spacing_voxels,
        first.debug_view,
    ) == (
        second.width,
        second.height,
        second.backend,
        second.spacing_voxels,
        second.debug_view,
    )
    return {
        "compatible": compatible,
        "bit_exact": compatible and first.payload == second.payload,
        "first_sha256": hashlib.sha256(first.payload).hexdigest(),
        "second_sha256": hashlib.sha256(second.payload).hexdigest(),
    }


def compare_reference(approximate: Capture, exact: Capture) -> dict[str, object]:
    compatible = (
        approximate.width,
        approximate.height,
        approximate.backend,
        approximate.spacing_voxels,
    ) == (
        exact.width,
        exact.height,
        exact.backend,
        exact.spacing_voxels,
    )
    if not compatible:
        return {"compatible": False}

    luminance_errors: list[float] = []
    luminance_overestimates: list[float] = []
    channel_errors: list[float] = []
    hit_mask_matches = True
    peak_error = (-1.0, 0, 0)
    peak_overestimate = (0.0, 0, 0)
    for index, (approx_pixel, exact_pixel) in enumerate(
        zip(PIXEL.iter_unpack(approximate.payload), PIXEL.iter_unpack(exact.payload))
    ):
        ar, ag, ab, ah = approx_pixel
        er, eg, eb, eh = exact_pixel
        hit_mask_matches = hit_mask_matches and ((ah > 0.5) == (eh > 0.5))
        if ah <= 0.5 or eh <= 0.5:
            continue
        rgb_error = (abs(ar - er), abs(ag - eg), abs(ab - eb))
        luminance_error = (
            0.2126 * rgb_error[0] + 0.7152 * rgb_error[1] + 0.0722 * rgb_error[2]
        )
        approximate_luminance = 0.2126 * ar + 0.7152 * ag + 0.0722 * ab
        exact_luminance = 0.2126 * er + 0.7152 * eg + 0.0722 * eb
        overestimate = max(0.0, approximate_luminance - exact_luminance)
        x = index % approximate.width
        y = index // approximate.width
        if luminance_error > peak_error[0]:
            peak_error = (luminance_error, x, y)
        if overestimate > peak_overestimate[0]:
            peak_overestimate = (overestimate, x, y)
        luminance_errors.append(luminance_error)
        luminance_overestimates.append(overestimate)
        channel_errors.append(max(rgb_error))
    luminance_errors.sort()
    luminance_overestimates.sort()
    channel_errors.sort()
    return {
        "compatible": True,
        "hit_mask_matches": hit_mask_matches,
        "sample_count": len(luminance_errors),
        "luminance_error_mean": (
            sum(luminance_errors) / len(luminance_errors) if luminance_errors else 0.0
        ),
        "luminance_error_p99": percentile(luminance_errors, 0.99),
        "luminance_error_max": luminance_errors[-1] if luminance_errors else 0.0,
        "luminance_error_peak_xy": [peak_error[1], peak_error[2]],
        "luminance_overestimate_mean": (
            sum(luminance_overestimates) / len(luminance_overestimates)
            if luminance_overestimates else 0.0
        ),
        "luminance_overestimate_p99": percentile(luminance_overestimates, 0.99),
        "luminance_overestimate_max": (
            luminance_overestimates[-1] if luminance_overestimates else 0.0
        ),
        "luminance_overestimate_peak_xy": [
            peak_overestimate[1], peak_overestimate[2]
        ],
        "channel_error_p99": percentile(channel_errors, 0.99),
        "channel_error_max": channel_errors[-1] if channel_errors else 0.0,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("capture", type=Path)
    parser.add_argument("--compare", type=Path)
    parser.add_argument("--reference", type=Path)
    parser.add_argument("--max-luminance", type=float)
    parser.add_argument("--min-luminance-p99", type=float)
    parser.add_argument("--max-reference-error-p99", type=float)
    parser.add_argument("--max-reference-overestimate-p99", type=float)
    args = parser.parse_args()

    first = load_capture(args.capture)
    report: dict[str, object] = {"capture": summarize(first)}
    exit_code = 0
    if args.compare is not None:
        comparison = compare(first, load_capture(args.compare))
        report["comparison"] = comparison
        if not comparison["bit_exact"]:
            exit_code = 1
    if args.reference is not None:
        reference = compare_reference(first, load_capture(args.reference))
        report["reference_comparison"] = reference
        if not reference["compatible"] or not reference.get("hit_mask_matches", False):
            exit_code = 1
        if (
            args.max_reference_error_p99 is not None
            and reference.get("luminance_error_p99", math.inf)
            > args.max_reference_error_p99
        ):
            exit_code = 1
        if (
            args.max_reference_overestimate_p99 is not None
            and reference.get("luminance_overestimate_p99", math.inf)
            > args.max_reference_overestimate_p99
        ):
            exit_code = 1
    if not report["capture"]["finite"] or report["capture"]["terrain_hit_count"] == 0:
        exit_code = 1
    if (
        args.max_luminance is not None
        and report["capture"]["luminance_max"] > args.max_luminance
    ):
        exit_code = 1
    if (
        args.min_luminance_p99 is not None
        and report["capture"]["luminance_p99"] < args.min_luminance_p99
    ):
        exit_code = 1
    print(json.dumps(report, indent=2, sort_keys=True))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
