#!/usr/bin/env python3
"""Validate DDGI full-atlas convergence curves and emit machine-readable provenance."""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from dataclasses import dataclass
from pathlib import Path


DEFAULT_CASES = ("sealed", "portal", "donor", "dogleg")
DEFAULT_SPACINGS = (32, 16)
VALIDATION_PATTERN = re.compile(
    r"transport=(?P<transport>\w+) iteration=(?P<iteration>\d+).*?"
    r"max_abs_rgb_delta=(?P<absolute>[0-9.eE+-]+) "
    r"max_rel_rgb_delta=(?P<relative>[0-9.eE+-]+) "
    r"non_finite=(?P<nonfinite>\d+) "
    r"negative_rgb_texels=(?P<negative>\d+) "
    r"valid_texels=(?P<valid>\d+) "
    r"scanned_stored_texels=(?P<scanned>\d+) "
    r"abs_threshold=(?P<absolute_threshold>[0-9.eE+-]+) "
    r"rel_threshold=(?P<relative_threshold>[0-9.eE+-]+) "
    r"consecutive_below=(?P<consecutive>\d+)/(?P<required>\d+) "
    r"hard_max=(?P<hard_max>\d+)"
)


@dataclass(frozen=True)
class Policy:
    absolute_threshold: float
    relative_threshold: float
    consecutive_iterations: int
    hard_max_iteration: int


def close(left: float, right: float) -> bool:
    return math.isclose(left, right, rel_tol=0.0, abs_tol=5.0e-8)


def parse_curve(console_path: Path) -> list[dict[str, object]]:
    records: list[dict[str, object]] = []
    for line in console_path.read_text().splitlines():
        if "[DDGI] full-atlas validated" not in line:
            continue
        match = VALIDATION_PATTERN.search(line)
        if match is None:
            raise ValueError(f"malformed full-atlas validation line in {console_path}: {line}")
        values = match.groupdict()
        records.append(
            {
                "transport_stage": values["transport"],
                "iteration": int(values["iteration"]),
                "max_absolute_rgb_delta": float(values["absolute"]),
                "max_relative_rgb_delta": float(values["relative"]),
                "nonfinite_count": int(values["nonfinite"]),
                "negative_rgb_texel_count": int(values["negative"]),
                "valid_texel_count": int(values["valid"]),
                "scanned_stored_texel_count": int(values["scanned"]),
                "absolute_threshold": float(values["absolute_threshold"]),
                "relative_threshold": float(values["relative_threshold"]),
                "consecutive_below_threshold": int(values["consecutive"]),
                "required_consecutive_iterations": int(values["required"]),
                "hard_max_iteration": int(values["hard_max"]),
            }
        )
    if not records:
        raise ValueError(f"no full-atlas validation records in {console_path}")
    return records


def validate_curve(
    case_name: str,
    spacing: int,
    records: list[dict[str, object]],
    analysis: dict[str, object],
    policy: Policy,
) -> dict[str, object]:
    capture = analysis.get("capture")
    if not isinstance(capture, dict):
        raise ValueError(f"{case_name} spacing {spacing}: analysis has no capture object")
    failures = analysis.get("validation_failures")
    if failures != []:
        raise ValueError(
            f"{case_name} spacing {spacing}: analyzer validation failures: {failures}"
        )
    if capture.get("transport_stage") != "converged":
        raise ValueError(f"{case_name} spacing {spacing}: capture is not converged")
    if capture.get("spacing_voxels") != spacing:
        raise ValueError(f"{case_name} spacing {spacing}: capture spacing mismatch")

    iterations = [int(record["iteration"]) for record in records]
    if iterations != list(range(iterations[-1] + 1)):
        raise ValueError(
            f"{case_name} spacing {spacing}: incomplete iteration sequence {iterations}"
        )
    expected_stages = ["SeedSky", "SingleBounce"] + [
        "Feedback"
    ] * max(0, len(records) - 2)
    stages = [str(record["transport_stage"]) for record in records]
    if stages != expected_stages:
        raise ValueError(
            f"{case_name} spacing {spacing}: unexpected transport sequence {stages}"
        )

    previous_consecutive = 0
    for record_index, record in enumerate(records):
        if not close(float(record["absolute_threshold"]), policy.absolute_threshold):
            raise ValueError(f"{case_name} spacing {spacing}: absolute policy drift")
        if not close(float(record["relative_threshold"]), policy.relative_threshold):
            raise ValueError(f"{case_name} spacing {spacing}: relative policy drift")
        if record["required_consecutive_iterations"] != policy.consecutive_iterations:
            raise ValueError(f"{case_name} spacing {spacing}: consecutive policy drift")
        if record["hard_max_iteration"] != policy.hard_max_iteration:
            raise ValueError(f"{case_name} spacing {spacing}: hard-max policy drift")
        if record["nonfinite_count"] != 0 or record["negative_rgb_texel_count"] != 0:
            raise ValueError(f"{case_name} spacing {spacing}: invalid atlas values")
        valid = int(record["valid_texel_count"])
        scanned = int(record["scanned_stored_texel_count"])
        if valid <= 0 or scanned <= 0 or scanned * 64 != valid * 100:
            raise ValueError(f"{case_name} spacing {spacing}: incomplete atlas coverage")
        if int(record["iteration"]) < 2:
            expected_consecutive = 0
        else:
            below = (
                float(record["max_absolute_rgb_delta"])
                <= policy.absolute_threshold
                and float(record["max_relative_rgb_delta"])
                <= policy.relative_threshold
            )
            expected_consecutive = previous_consecutive + 1 if below else 0
            previous_consecutive = expected_consecutive
        if record["consecutive_below_threshold"] != expected_consecutive:
            raise ValueError(
                f"{case_name} spacing {spacing}: invalid consecutive classification at "
                f"S{record['iteration']}"
            )
        if (
            record_index < len(records) - 1
            and expected_consecutive >= policy.consecutive_iterations
        ):
            raise ValueError(
                f"{case_name} spacing {spacing}: curve continued after convergence at "
                f"S{record['iteration']}"
            )

    final = records[-1]
    final_iteration = int(final["iteration"])
    if final_iteration > policy.hard_max_iteration:
        raise ValueError(f"{case_name} spacing {spacing}: converged after hard max")
    if final["consecutive_below_threshold"] != policy.consecutive_iterations:
        raise ValueError(f"{case_name} spacing {spacing}: terminal curve lacks two passes")
    if capture.get("transport_iteration") != final_iteration:
        raise ValueError(f"{case_name} spacing {spacing}: capture iteration mismatch")
    if not close(float(capture["max_abs_delta"]), float(final["max_absolute_rgb_delta"])):
        raise ValueError(f"{case_name} spacing {spacing}: capture absolute delta mismatch")
    if not close(float(capture["max_rel_delta"]), float(final["max_relative_rgb_delta"])):
        raise ValueError(f"{case_name} spacing {spacing}: capture relative delta mismatch")

    return {
        "case": case_name,
        "spacing_voxels": spacing,
        "qualified": True,
        "final_iteration": final_iteration,
        "iterations_before_hard_max": policy.hard_max_iteration - final_iteration,
        "final_max_absolute_rgb_delta": float(final["max_absolute_rgb_delta"]),
        "final_max_relative_rgb_delta": float(final["max_relative_rgb_delta"]),
        "absolute_threshold_margin": policy.absolute_threshold
        - float(final["max_absolute_rgb_delta"]),
        "relative_threshold_margin": policy.relative_threshold
        - float(final["max_relative_rgb_delta"]),
        "consecutive_below_threshold": int(final["consecutive_below_threshold"]),
        "iterations": records,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--absolute-threshold", type=float, required=True)
    parser.add_argument("--relative-threshold", type=float, required=True)
    parser.add_argument("--consecutive-iterations", type=int, required=True)
    parser.add_argument("--hard-max-iteration", type=int, required=True)
    parser.add_argument("--cases", nargs="+", default=list(DEFAULT_CASES))
    parser.add_argument("--spacings", nargs="+", type=int, default=list(DEFAULT_SPACINGS))
    args = parser.parse_args()
    policy = Policy(
        args.absolute_threshold,
        args.relative_threshold,
        args.consecutive_iterations,
        args.hard_max_iteration,
    )

    try:
        curves = []
        for spacing in args.spacings:
            for case_name in args.cases:
                stem = f"{case_name}-spacing{spacing}-converged-forward"
                console_path = args.run_dir / f"{stem}.console.log"
                analysis_path = args.run_dir / f"{stem}.analysis.json"
                analysis = json.loads(analysis_path.read_text())
                curve = validate_curve(
                    case_name,
                    spacing,
                    parse_curve(console_path),
                    analysis,
                    policy,
                )
                curve["capture_analysis"] = analysis_path.name
                curve["console_log"] = console_path.name
                curves.append(curve)
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"DDGI convergence provenance validation failed: {error}", file=sys.stderr)
        return 1

    result = {
        "schema_version": 1,
        "qualified": True,
        "matrix": {
            "cases": args.cases,
            "spacings_voxels": args.spacings,
            "curve_count": len(curves),
        },
        "policy": {
            "max_absolute_rgb_delta": policy.absolute_threshold,
            "max_relative_rgb_delta": policy.relative_threshold,
            "consecutive_iterations": policy.consecutive_iterations,
            "hard_max_iteration": policy.hard_max_iteration,
        },
        "curves": curves,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(
        f"[DDGI_CONVERGENCE] PASS curves={len(curves)} output={args.output} "
        f"hard_max={policy.hard_max_iteration}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
