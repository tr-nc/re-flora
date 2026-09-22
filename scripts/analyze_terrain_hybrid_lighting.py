#!/usr/bin/env python3
"""Check permanent lighting in the fixed thin-voxels ordinary-terrain scene.

Usage: python3 scripts/analyze_terrain_hybrid_lighting.py target/terrain-hybrid/linear
Generate hybrid.rfirr, hybrid.log and hybrid.gui.toml with
check_terrain_hybrid_lighting.py --irradiance. Uniform rock albedo lets nine isolated
cells be compared with a broad control face without a retired rendering mode.
Historical directories containing A.rfirr/B.rfirr and logs remain readable;
--b overrides the historical B capture only. No off mode is run or emulated.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import statistics
from pathlib import Path

import tomllib
from analyze_environment_irradiance_capture import PIXEL, load_capture

# Identified receivers in the fixture, deliberately not a search for any pixels
# that happen to match the desired result. A moved fixture must update the test.
ISOLATED = tuple((x, y, 300) for y in (186, 194, 202) for x in (237, 242, 247))
CONTROL = (286, 193, 305)


def samples(path: Path):
    capture = load_capture(path)
    result = {cell: [] for cell in (*ISOLATED, CONTROL)}
    for direct, receiver, shadow in zip(
        PIXEL.iter_unpack(capture.direct_light_payload),
        PIXEL.iter_unpack(capture.terrain_shadow_receiver_payload),
        PIXEL.iter_unpack(capture.direct_sun_shadow_payload),
        strict=True,
    ):
        if not direct[3]:
            continue
        if not all(math.isfinite(v) for v in (*direct, *receiver, *shadow)):
            raise ValueError("nonfinite GPU lighting output")
        cell = tuple(round(v * 256 - 0.5) for v in receiver[:3])
        if cell not in result or shadow[3] < 0.9:
            continue
        # Divide out the recorded shadow visibility, not albedo or solar energy.
        result[cell].append(sum(direct[:3]) / shadow[3])
    if min(map(len, result.values())) < 4:
        raise ValueError(
            f"missing identified sunlit receiver samples: {[(c, len(v)) for c, v in result.items()]}"
        )
    return result, (capture.width, capture.height)


def sun_direction(log_path: Path):
    matches = re.findall(r"sun_direction=Vec3\(([^)]+)\)", log_path.read_text())
    if not matches:
        raise ValueError("missing actual solar direction")
    return tuple(map(float, matches[0].split(",")))


def measure_current(root: Path):
    config = tomllib.loads((root / "hybrid.gui.toml").read_text())
    params = {
        p["id"]: p["data"]["value"]
        for s in config["section"]
        for p in s["param"]
        if "value" in p["data"]
    }
    if (
        params.get("terrain_rock_strength") != 0
        or params.get("path_tracing_reference") is not False
    ):
        raise ValueError(
            "requires uniform rock albedo and normal shading; recapture with check_terrain_hybrid_lighting.py --irradiance"
        )
    values, size = samples(root / "hybrid.rfirr")
    x, y, z = sun_direction(root / "hybrid.log")
    # The control is an interior +Z face, oct16-quantized to normalize(1,1,253).
    control_cosine = (x + y + 253 * z) / math.sqrt(253**2 + 2)
    control = statistics.median(values[CONTROL])
    if y <= 0 or control_cosine <= 0 or control <= 0:
        raise ValueError("fixture requires above-horizon sun and a sunlit +Z control")
    expected = (abs(x) + abs(y) + abs(z)) / (6 * control_cosine)
    ratios = {str(c): statistics.median(values[c]) / control for c in ISOLATED}
    return {
        "verdict": "GREEN"
        if all(abs(r - expected) < 0.005 for r in ratios.values())
        else "RED",
        "resolution": size,
        "expected_isolated_to_control_ratio": expected,
        "measured_ratios": ratios,
        "sample_counts": {str(c): len(v) for c, v in values.items()},
    }


def measure(root: Path, b_path: Path | None = None):
    if b_path is None and any(
        (root / f"hybrid.{suffix}").exists() for suffix in ("rfirr", "log", "gui.toml")
    ):
        # A partial new capture must fail, not silently validate stale A/B files.
        return measure_current(root)
    return measure_legacy(root, b_path)


def measure_legacy(root: Path, b_path: Path | None = None):
    a, a_size = samples(root / "A.rfirr")
    b, b_size = samples(b_path or root / "B.rfirr")
    if a_size != b_size:
        raise ValueError("A/B capture resolution changed")
    directions = [
        sun_direction(root / "A.log"),
        sun_direction(b_path.with_suffix(".log") if b_path else root / "B.log"),
    ]
    if directions[0] != directions[1]:
        raise ValueError("A/B lighting changed")
    x, y, z = directions[0]
    # The legacy upward fallback is oct16-quantized to normalize(0,254,-1).
    original_cosine = (254 * y - z) / math.sqrt(254**2 + 1)
    if original_cosine <= 0:
        raise ValueError("fixture must have an above-horizon sun")
    expected = (abs(x) + abs(y) + abs(z)) / (6 * original_cosine)
    ratios = {
        str(cell): statistics.median(b[cell]) / statistics.median(a[cell])
        for cell in ISOLATED
    }
    control = statistics.median(b[CONTROL]) / statistics.median(a[CONTROL])
    passed = (
        all(abs(ratio - expected) < 0.005 for ratio in ratios.values())
        and abs(control - 1) < 0.005
    )
    return {
        "verdict": "GREEN" if passed else "RED",
        "resolution": a_size,
        "expected_isolated_solar_ratio": expected,
        "measured_ratios": ratios,
        "control_ratio": control,
        "sample_counts": {str(c): [len(a[c]), len(b[c])] for c in a},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument(
        "--b",
        type=Path,
        help="Historical A/B directories only: override the B capture and its adjacent .log",
    )
    args = parser.parse_args()
    result = measure(args.directory, args.b)
    print(json.dumps(result, indent=2))
    return 0 if result["verdict"] == "GREEN" else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        raise SystemExit(str(error)) from error
