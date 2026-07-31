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
HEADER = struct.Struct("<8s6I")
PIXEL = struct.Struct("<4f")


@dataclass(frozen=True)
class Capture:
    path: Path
    width: int
    height: int
    backend: int
    spacing_voxels: int
    payload: bytes

    @property
    def sample_count(self) -> int:
        return self.width * self.height


def load_capture(path: Path) -> Capture:
    data = path.read_bytes()
    if len(data) < HEADER.size:
        raise ValueError(f"{path}: truncated header")
    magic, version, width, height, channels, backend, spacing = HEADER.unpack_from(data)
    if magic != MAGIC:
        raise ValueError(f"{path}: invalid magic {magic!r}")
    if version != 1:
        raise ValueError(f"{path}: unsupported version {version}")
    if channels != 4:
        raise ValueError(f"{path}: expected four float channels, got {channels}")
    payload = data[HEADER.size :]
    expected = width * height * PIXEL.size
    if len(payload) != expected:
        raise ValueError(f"{path}: payload is {len(payload)} bytes, expected {expected}")
    return Capture(path, width, height, backend, spacing, payload)


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
    ) == (
        second.width,
        second.height,
        second.backend,
        second.spacing_voxels,
    )
    return {
        "compatible": compatible,
        "bit_exact": compatible and first.payload == second.payload,
        "first_sha256": hashlib.sha256(first.payload).hexdigest(),
        "second_sha256": hashlib.sha256(second.payload).hexdigest(),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("capture", type=Path)
    parser.add_argument("--compare", type=Path)
    args = parser.parse_args()

    first = load_capture(args.capture)
    report: dict[str, object] = {"capture": summarize(first)}
    exit_code = 0
    if args.compare is not None:
        comparison = compare(first, load_capture(args.compare))
        report["comparison"] = comparison
        if not comparison["bit_exact"]:
            exit_code = 1
    if not report["capture"]["finite"] or report["capture"]["terrain_hit_count"] == 0:
        exit_code = 1
    print(json.dumps(report, indent=2, sort_keys=True))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
