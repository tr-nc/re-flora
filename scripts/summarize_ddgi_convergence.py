#!/usr/bin/env python3
"""Validate DDGI temporal epoch curves and emit machine-readable provenance."""

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
    r"geometry_revision=(?P<geometry>\d+) "
    r"radiance_revision=(?P<radiance>\d+) "
    r"spacing_voxels=(?P<spacing>\d+) "
    r"state=(?P<state>\w+) update_epoch=(?P<epoch>\d+).*?"
    r"max_abs_rgb_delta=(?P<absolute>[0-9.eE+-]+) "
    r"max_rel_rgb_delta=(?P<relative>[0-9.eE+-]+) "
    r"non_finite=(?P<nonfinite>\d+) "
    r"negative_rgb_texels=(?P<negative>\d+) "
    r"valid_texels=(?P<valid>\d+) "
    r"scanned_stored_texels=(?P<scanned>\d+) "
    r"abs_threshold=(?P<absolute_threshold>[0-9.eE+-]+) "
    r"rel_threshold=(?P<relative_threshold>[0-9.eE+-]+) "
    r"consecutive_below=(?P<consecutive>\d+)/(?P<required>\d+)"
)
TERMINAL_PATTERN = re.compile(
    r"transport converged .*update_epoch=(?P<epoch>\d+).*reason=(?P<reason>\w+)"
)


@dataclass(frozen=True)
class Policy:
    absolute_threshold: float
    relative_threshold: float
    consecutive_epochs: int
    minimum_epoch_count: int
    maximum_update_epoch: int


def close(left: float, right: float) -> bool:
    return math.isclose(left, right, rel_tol=0.0, abs_tol=5.0e-8)


def parse_curve(console_path: Path) -> tuple[list[dict[str, object]], str]:
    records: list[dict[str, object]] = []
    terminal_reason: str | None = None
    for line in console_path.read_text().splitlines():
        if "[DDGI] full-atlas validated" in line:
            match = VALIDATION_PATTERN.search(line)
            if match is None:
                raise ValueError(
                    f"malformed full-atlas validation line in {console_path}: {line}"
                )
            values = match.groupdict()
            records.append(
                {
                    "geometry_revision": int(values["geometry"]),
                    "radiance_revision": int(values["radiance"]),
                    "spacing_voxels": int(values["spacing"]),
                    "state": values["state"],
                    "update_epoch": int(values["epoch"]),
                    "max_absolute_rgb_delta": float(values["absolute"]),
                    "max_relative_rgb_delta": float(values["relative"]),
                    "nonfinite_count": int(values["nonfinite"]),
                    "negative_rgb_texel_count": int(values["negative"]),
                    "valid_texel_count": int(values["valid"]),
                    "scanned_stored_texel_count": int(values["scanned"]),
                    "absolute_threshold": float(values["absolute_threshold"]),
                    "relative_threshold": float(values["relative_threshold"]),
                    "consecutive_below_threshold": int(values["consecutive"]),
                    "required_consecutive_epochs": int(values["required"]),
                }
            )
        if "[DDGI] transport converged" in line:
            match = TERMINAL_PATTERN.search(line)
            if match is None:
                raise ValueError(f"malformed convergence line in {console_path}: {line}")
            terminal_reason = match.group("reason")
    if not records:
        raise ValueError(f"no full-atlas validation records in {console_path}")
    if terminal_reason is None:
        raise ValueError(f"no terminal convergence record in {console_path}")
    return records, terminal_reason


def validate_curve(
    case_name: str,
    spacing: int,
    records: list[dict[str, object]],
    terminal_reason: str,
    analysis: dict[str, object],
    policy: Policy,
) -> dict[str, object]:
    capture = analysis.get("capture")
    if not isinstance(capture, dict):
        raise ValueError(f"{case_name} spacing {spacing}: analysis has no capture object")
    if analysis.get("validation_failures") != []:
        raise ValueError(
            f"{case_name} spacing {spacing}: analyzer failures: "
            f"{analysis.get('validation_failures')}"
        )
    if capture.get("lifecycle_state") != "converged":
        raise ValueError(f"{case_name} spacing {spacing}: capture is not converged")
    if capture.get("spacing_voxels") != spacing:
        raise ValueError(f"{case_name} spacing {spacing}: capture spacing mismatch")
    geometry_revision = capture.get("geometry_revision")
    radiance_revision = capture.get("radiance_revision")
    records = [
        record
        for record in records
        if record["geometry_revision"] == geometry_revision
        and record["radiance_revision"] == radiance_revision
        and record["spacing_voxels"] == spacing
    ]
    if not records:
        raise ValueError(
            f"{case_name} spacing {spacing}: no curve for captured geometry/radiance revision"
        )

    epochs = [int(record["update_epoch"]) for record in records]
    if epochs != list(range(epochs[-1] + 1)):
        raise ValueError(f"{case_name} spacing {spacing}: incomplete epoch sequence {epochs}")

    previous_consecutive = 0
    first_threshold_epoch: int | None = None
    for record in records:
        epoch = int(record["update_epoch"])
        if record["state"] != "Converging":
            raise ValueError(f"{case_name} spacing {spacing}: destination state drift")
        if not close(float(record["absolute_threshold"]), policy.absolute_threshold):
            raise ValueError(f"{case_name} spacing {spacing}: absolute policy drift")
        if not close(float(record["relative_threshold"]), policy.relative_threshold):
            raise ValueError(f"{case_name} spacing {spacing}: relative policy drift")
        if record["required_consecutive_epochs"] != policy.consecutive_epochs:
            raise ValueError(f"{case_name} spacing {spacing}: consecutive policy drift")
        if record["nonfinite_count"] != 0 or record["negative_rgb_texel_count"] != 0:
            raise ValueError(f"{case_name} spacing {spacing}: invalid atlas values")
        valid = int(record["valid_texel_count"])
        scanned = int(record["scanned_stored_texel_count"])
        if valid <= 0 or scanned <= 0 or scanned * 64 != valid * 100:
            raise ValueError(f"{case_name} spacing {spacing}: incomplete atlas coverage")

        if epoch == 0:
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
                f"{case_name} spacing {spacing}: invalid consecutive classification at e{epoch}"
            )
        if (
            epoch + 1 >= policy.minimum_epoch_count
            and expected_consecutive >= policy.consecutive_epochs
            and first_threshold_epoch is None
        ):
            first_threshold_epoch = epoch

    final = records[-1]
    final_epoch = int(final["update_epoch"])
    expected_reason = (
        "Threshold" if first_threshold_epoch == final_epoch else "SampleBudget"
    )
    if first_threshold_epoch is not None and first_threshold_epoch != final_epoch:
        raise ValueError(f"{case_name} spacing {spacing}: curve continued after threshold sleep")
    if first_threshold_epoch is None and final_epoch != policy.maximum_update_epoch:
        raise ValueError(f"{case_name} spacing {spacing}: sample budget ended at e{final_epoch}")
    if terminal_reason != expected_reason:
        raise ValueError(
            f"{case_name} spacing {spacing}: terminal reason {terminal_reason}, "
            f"expected {expected_reason}"
        )
    if capture.get("update_epoch") != final_epoch:
        raise ValueError(f"{case_name} spacing {spacing}: capture epoch mismatch")
    if not close(float(capture["max_abs_delta"]), float(final["max_absolute_rgb_delta"])):
        raise ValueError(f"{case_name} spacing {spacing}: capture absolute delta mismatch")
    if not close(float(capture["max_rel_delta"]), float(final["max_relative_rgb_delta"])):
        raise ValueError(f"{case_name} spacing {spacing}: capture relative delta mismatch")

    return {
        "case": case_name,
        "spacing_voxels": spacing,
        "qualified": True,
        "final_update_epoch": final_epoch,
        "terminal_reason": terminal_reason,
        "final_max_absolute_rgb_delta": float(final["max_absolute_rgb_delta"]),
        "final_max_relative_rgb_delta": float(final["max_relative_rgb_delta"]),
        "epochs": records,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--absolute-threshold", type=float, required=True)
    parser.add_argument("--relative-threshold", type=float, required=True)
    parser.add_argument("--consecutive-epochs", type=int, required=True)
    parser.add_argument("--minimum-epoch-count", type=int, required=True)
    parser.add_argument("--maximum-update-epoch", type=int, required=True)
    parser.add_argument("--cases", nargs="+", default=list(DEFAULT_CASES))
    parser.add_argument("--spacings", nargs="+", type=int, default=list(DEFAULT_SPACINGS))
    args = parser.parse_args()
    policy = Policy(
        args.absolute_threshold,
        args.relative_threshold,
        args.consecutive_epochs,
        args.minimum_epoch_count,
        args.maximum_update_epoch,
    )

    try:
        curves = []
        for spacing in args.spacings:
            for case_name in args.cases:
                stem = f"{case_name}-spacing{spacing}-converged-forward"
                console_path = args.run_dir / f"{stem}.console.log"
                analysis_path = args.run_dir / f"{stem}.analysis.json"
                records, terminal_reason = parse_curve(console_path)
                curve = validate_curve(
                    case_name,
                    spacing,
                    records,
                    terminal_reason,
                    json.loads(analysis_path.read_text()),
                    policy,
                )
                curve["capture_analysis"] = analysis_path.name
                curve["console_log"] = console_path.name
                curves.append(curve)
    except (OSError, ValueError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"DDGI convergence provenance validation failed: {error}", file=sys.stderr)
        return 1

    result = {
        "schema_version": 2,
        "qualified": True,
        "matrix": {
            "cases": args.cases,
            "spacings_voxels": args.spacings,
            "curve_count": len(curves),
        },
        "policy": {
            "max_absolute_rgb_delta": policy.absolute_threshold,
            "max_relative_rgb_delta": policy.relative_threshold,
            "consecutive_epochs": policy.consecutive_epochs,
            "minimum_epoch_count": policy.minimum_epoch_count,
            "maximum_update_epoch": policy.maximum_update_epoch,
        },
        "curves": curves,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(
        f"[DDGI_CONVERGENCE] PASS curves={len(curves)} output={args.output} "
        f"maximum_update_epoch={policy.maximum_update_epoch}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
