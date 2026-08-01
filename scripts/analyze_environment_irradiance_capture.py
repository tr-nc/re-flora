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
HEADER_V3 = struct.Struct("<8s10I2Q4IQI2f2I")
PIXEL = struct.Struct("<4f")
UNKNOWN_U32 = 0xFFFFFFFF
UNKNOWN_U64 = 0xFFFFFFFFFFFFFFFF
UNKNOWN_DELTA = -1.0

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
TRANSPORT_STAGE_LABELS = {
    1: "seed-sky",
    2: "single-bounce",
    3: "feedback",
    4: "converged",
    5: "non-converged",
}
PUBLICATION_STATE_LABELS = {
    0: "unpublished",
    1: "published",
}


@dataclass(frozen=True)
class Capture:
    path: Path
    version: int
    width: int
    height: int
    backend: int
    spacing_voxels: int
    debug_view: int
    payload: bytes
    world_payload: bytes = b""
    plane_count: int = 1
    geometry_revision: int | None = None
    radiance_revision: int | None = None
    radiance_model_identity: int | None = None
    token_serial: int | None = None
    transport_stage: int | None = None
    transport_iteration: int | None = None
    source_stage: int | None = None
    source_iteration: int | None = None
    source_identity: int | None = None
    publication_state: int | None = None
    max_abs_delta: float | None = None
    max_rel_delta: float | None = None
    nonfinite_count: int | None = None
    valid_count: int | None = None

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
        plane_count = 1
        metadata: tuple[object, ...] = (None,) * 14
    elif version == 2:
        if len(data) < HEADER_V2.size:
            raise ValueError(f"{path}: truncated v2 header")
        _, _, width, height, channels, backend, spacing, debug_view = HEADER_V2.unpack_from(data)
        header_size = HEADER_V2.size
        plane_count = 1
        metadata = (None,) * 14
    elif version == 3:
        if len(data) < HEADER_V3.size:
            raise ValueError(f"{path}: truncated v3 header")
        (
            _,
            _,
            width,
            height,
            channels,
            backend,
            spacing,
            debug_view,
            plane_count,
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
        ) = HEADER_V3.unpack_from(data)
        header_size = HEADER_V3.size
        metadata = (
            None if geometry_revision == UNKNOWN_U32 else geometry_revision,
            None if radiance_revision == UNKNOWN_U32 else radiance_revision,
            None
            if radiance_model_identity == UNKNOWN_U64
            else radiance_model_identity,
            None if token_serial == UNKNOWN_U64 else token_serial,
            None if transport_stage == UNKNOWN_U32 else transport_stage,
            None if transport_iteration == UNKNOWN_U32 else transport_iteration,
            None if source_stage == UNKNOWN_U32 else source_stage,
            None if source_iteration == UNKNOWN_U32 else source_iteration,
            None if source_identity == UNKNOWN_U64 else source_identity,
            None if publication_state == UNKNOWN_U32 else publication_state,
            None if max_abs_delta == UNKNOWN_DELTA else max_abs_delta,
            None if max_rel_delta == UNKNOWN_DELTA else max_rel_delta,
            None if nonfinite_count == UNKNOWN_U32 else nonfinite_count,
            None if valid_count == UNKNOWN_U32 else valid_count,
        )
    else:
        raise ValueError(f"{path}: unsupported version {version}")
    if channels != 4:
        raise ValueError(f"{path}: expected four float channels, got {channels}")
    if plane_count not in (1, 2):
        raise ValueError(f"{path}: expected one or two float4 planes, got {plane_count}")
    payload = data[header_size:]
    plane_size = width * height * PIXEL.size
    expected = plane_size * plane_count
    if len(payload) != expected:
        raise ValueError(f"{path}: payload is {len(payload)} bytes, expected {expected}")
    return Capture(
        path,
        version,
        width,
        height,
        backend,
        spacing,
        debug_view,
        payload[:plane_size],
        payload[plane_size:],
        plane_count,
        *metadata,
    )


def percentile(sorted_values: list[float], fraction: float) -> float:
    if not sorted_values:
        return 0.0
    index = math.ceil(fraction * len(sorted_values)) - 1
    return sorted_values[max(0, min(index, len(sorted_values) - 1))]


def summarize(
    capture: Capture,
    world_roi: tuple[float, float, float, float, float, float] | None = None,
) -> dict[str, object]:
    luminances: list[float] = []
    finite = True
    terrain_hit_count = 0
    rgb_abs_max = 0.0
    rgb_nonzero_count = 0
    rgb_channel_min = [math.inf, math.inf, math.inf]
    rgb_channel_negative_count = [0, 0, 0]
    roi_terrain_hit_count = 0
    channel_abs_max = [0.0, 0.0, 0.0]
    channel_nonzero_count = [0, 0, 0]
    world_min = [math.inf, math.inf, math.inf]
    world_max = [-math.inf, -math.inf, -math.inf]
    exact_sun_visibilities: list[float] = []
    world_pixels = (
        list(PIXEL.iter_unpack(capture.world_payload))
        if capture.world_payload
        else [None] * capture.sample_count
    )
    for (red, green, blue, hit), world_pixel in zip(
        PIXEL.iter_unpack(capture.payload), world_pixels
    ):
        rgb = (red, green, blue)
        finite_rgb = all(math.isfinite(value) for value in rgb)
        finite = finite and finite_rgb and math.isfinite(hit)
        position: tuple[float, float, float] | None = None
        exact_sun_visibility: float | None = None
        if world_pixel is not None:
            world_x, world_y, world_z, exact_sun_visibility = world_pixel
            position = (world_x, world_y, world_z)
            finite = finite and all(
                math.isfinite(value) for value in (*position, exact_sun_visibility)
            )
        if hit > 0.5:
            terrain_hit_count += 1
            if finite_rgb:
                luminances.append(0.2126 * red + 0.7152 * green + 0.0722 * blue)
                rgb_abs_max = max(rgb_abs_max, *(abs(value) for value in rgb))
                if any(value != 0.0 for value in rgb):
                    rgb_nonzero_count += 1
                for channel, value in enumerate(rgb):
                    rgb_channel_min[channel] = min(rgb_channel_min[channel], value)
                    if value < 0.0:
                        rgb_channel_negative_count[channel] += 1
            in_roi = world_roi is None
            if position is not None and world_roi is not None:
                min_x, min_y, min_z, max_x, max_y, max_z = world_roi
                in_roi = (
                    min_x <= position[0] <= max_x
                    and min_y <= position[1] <= max_y
                    and min_z <= position[2] <= max_z
                )
            if in_roi:
                roi_terrain_hit_count += 1
                if finite_rgb:
                    for channel, value in enumerate(rgb):
                        channel_abs_max[channel] = max(
                            channel_abs_max[channel], abs(value)
                        )
                        if value != 0.0:
                            channel_nonzero_count[channel] += 1
                if position is not None:
                    for axis, value in enumerate(position):
                        world_min[axis] = min(world_min[axis], value)
                        world_max[axis] = max(world_max[axis], value)
                if exact_sun_visibility is not None and math.isfinite(
                    exact_sun_visibility
                ):
                    exact_sun_visibilities.append(exact_sun_visibility)
    luminances.sort()
    has_world_positions = world_min[0] != math.inf
    return {
        "path": str(capture.path),
        "version": capture.version,
        "width": capture.width,
        "height": capture.height,
        "backend": capture.backend,
        "spacing_voxels": capture.spacing_voxels,
        "debug_view": DEBUG_VIEW_LABELS.get(capture.debug_view, capture.debug_view),
        "sample_count": capture.sample_count,
        "terrain_hit_count": terrain_hit_count,
        "finite": finite,
        "rgb_abs_max": rgb_abs_max,
        "rgb_nonzero_count": rgb_nonzero_count,
        "rgb_channel_min": (
            rgb_channel_min if rgb_channel_min[0] != math.inf else None
        ),
        "rgb_channel_negative_count": rgb_channel_negative_count,
        "rgb_channel_abs_max": channel_abs_max,
        "rgb_channel_nonzero_count": channel_nonzero_count,
        "luminance_mean": sum(luminances) / len(luminances) if luminances else 0.0,
        "luminance_p99": percentile(luminances, 0.99),
        "luminance_max": luminances[-1] if luminances else 0.0,
        "world_roi": list(world_roi) if world_roi is not None else None,
        "world_roi_terrain_hit_count": roi_terrain_hit_count,
        "world_position_min": world_min if has_world_positions else None,
        "world_position_max": world_max if has_world_positions else None,
        "exact_direct_sun_visibility_mean": (
            sum(exact_sun_visibilities) / len(exact_sun_visibilities)
            if exact_sun_visibilities
            else None
        ),
        "exact_direct_sun_visibility_min": (
            min(exact_sun_visibilities) if exact_sun_visibilities else None
        ),
        "exact_direct_sun_visibility_max": (
            max(exact_sun_visibilities) if exact_sun_visibilities else None
        ),
        "geometry_revision": capture.geometry_revision,
        "radiance_revision": capture.radiance_revision,
        "radiance_model_identity": capture.radiance_model_identity,
        "token_serial": capture.token_serial,
        "transport_stage": TRANSPORT_STAGE_LABELS.get(
            capture.transport_stage, capture.transport_stage
        ),
        "transport_iteration": capture.transport_iteration,
        "source_stage": TRANSPORT_STAGE_LABELS.get(
            capture.source_stage, capture.source_stage
        ),
        "source_iteration": capture.source_iteration,
        "source_identity": capture.source_identity,
        "publication_state": PUBLICATION_STATE_LABELS.get(
            capture.publication_state, capture.publication_state
        ),
        "max_abs_delta": capture.max_abs_delta,
        "max_rel_delta": capture.max_rel_delta,
        "header_nonfinite_count": capture.nonfinite_count,
        "header_valid_count": capture.valid_count,
        "payload_sha256": hashlib.sha256(capture.payload).hexdigest(),
    }


def metadata_mismatches(first: Capture, second: Capture) -> list[str]:
    if first.version < 3 and second.version < 3:
        return []
    fields = (
        "version",
        "plane_count",
        "geometry_revision",
        "radiance_revision",
        "radiance_model_identity",
        "token_serial",
        "transport_stage",
        "transport_iteration",
        "source_stage",
        "source_iteration",
        "source_identity",
        "publication_state",
    )
    return [
        field for field in fields if getattr(first, field) != getattr(second, field)
    ]


def compare(first: Capture, second: Capture) -> dict[str, object]:
    base_compatible = (
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
    mismatches = metadata_mismatches(first, second)
    compatible = base_compatible and not mismatches
    return {
        "compatible": compatible,
        "metadata_mismatches": mismatches,
        "bit_exact": (
            compatible
            and first.payload == second.payload
            and first.world_payload == second.world_payload
        ),
        "first_sha256": hashlib.sha256(first.payload).hexdigest(),
        "second_sha256": hashlib.sha256(second.payload).hexdigest(),
    }


def compare_reference(approximate: Capture, exact: Capture) -> dict[str, object]:
    base_compatible = (
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
    mismatches = metadata_mismatches(approximate, exact)
    compatible = base_compatible and not mismatches
    if not compatible:
        return {"compatible": False, "metadata_mismatches": mismatches}

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
        "metadata_mismatches": [],
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
    parser.add_argument("--require-zero-rgb", action="store_true")
    parser.add_argument("--require-nonnegative-rgb", action="store_true")
    parser.add_argument("--min-luminance-p99", type=float)
    parser.add_argument("--max-reference-error-p99", type=float)
    parser.add_argument("--max-reference-overestimate-p99", type=float)
    parser.add_argument("--world-roi", type=float, nargs=6)
    parser.add_argument("--expect-geometry-revision", type=int)
    parser.add_argument("--expect-radiance-revision", type=int)
    parser.add_argument(
        "--expect-transport-stage", choices=tuple(TRANSPORT_STAGE_LABELS.values())
    )
    parser.add_argument("--expect-transport-iteration", type=int)
    parser.add_argument(
        "--expect-source-stage", choices=tuple(TRANSPORT_STAGE_LABELS.values())
    )
    parser.add_argument("--expect-source-iteration", type=int)
    parser.add_argument("--expect-source-identity", type=int)
    parser.add_argument(
        "--expect-publication-state", choices=tuple(PUBLICATION_STATE_LABELS.values())
    )
    parser.add_argument("--convergence-max-abs-delta", type=float)
    parser.add_argument("--convergence-max-rel-delta", type=float)
    args = parser.parse_args()

    first = load_capture(args.capture)
    capture_summary = summarize(
        first, tuple(args.world_roi) if args.world_roi is not None else None
    )
    failures: list[str] = []
    report: dict[str, object] = {
        "capture": capture_summary,
        "validation_failures": failures,
    }
    exit_code = 0

    def expect(field: str, expected: object) -> None:
        if expected is None:
            return
        actual = capture_summary[field]
        if actual != expected:
            failures.append(f"{field}: expected {expected}, got {actual}")

    expect("geometry_revision", args.expect_geometry_revision)
    expect("radiance_revision", args.expect_radiance_revision)
    expect("transport_stage", args.expect_transport_stage)
    expect("transport_iteration", args.expect_transport_iteration)
    expect("source_stage", args.expect_source_stage)
    expect("source_iteration", args.expect_source_iteration)
    expect("source_identity", args.expect_source_identity)
    expect("publication_state", args.expect_publication_state)
    if first.nonfinite_count is not None:
        expect("header_nonfinite_count", 0)
    if capture_summary["transport_stage"] == "converged":
        for field, threshold in (
            ("max_abs_delta", args.convergence_max_abs_delta),
            ("max_rel_delta", args.convergence_max_rel_delta),
        ):
            if threshold is None:
                continue
            actual = capture_summary[field]
            if actual is None:
                failures.append(
                    f"{field}: converged value is unknown; threshold is {threshold:g}"
                )
            elif actual > threshold:
                failures.append(
                    f"{field}: converged value {actual:g} exceeds {threshold:g}"
                )
    if failures:
        exit_code = 1
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
    if not capture_summary["finite"]:
        failures.append("payload contains nonfinite values")
        exit_code = 1
    if capture_summary["terrain_hit_count"] == 0:
        failures.append("capture contains no terrain hits")
        exit_code = 1
    if (
        args.max_luminance is not None
        and capture_summary["luminance_max"] > args.max_luminance
    ):
        exit_code = 1
    if args.require_zero_rgb and capture_summary["rgb_nonzero_count"] != 0:
        exit_code = 1
    if args.require_nonnegative_rgb and any(
        count != 0 for count in capture_summary["rgb_channel_negative_count"]
    ):
        failures.append("terrain-hit RGB contains negative channel values")
        exit_code = 1
    if (
        args.min_luminance_p99 is not None
        and capture_summary["luminance_p99"] < args.min_luminance_p99
    ):
        exit_code = 1
    print(json.dumps(report, indent=2, sort_keys=True))
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
