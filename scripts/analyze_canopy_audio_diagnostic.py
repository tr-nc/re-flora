#!/usr/bin/env python3
"""Validate deterministic distributed-canopy acoustic telemetry from a Re: Flora run log."""

from __future__ import annotations

import argparse
import math
import re
import sys
from dataclasses import dataclass
from pathlib import Path


MIN_WOOD_CLEARANCE_VOXELS = 2.0
MAX_ADJACENT_GAIN_STEP_DB = 12.0
MAX_HOLD_GAIN_STEP_DB = 3.0
MAX_RAW_SYMMETRY_ERROR_DB = 4.0
MAX_FILTERED_SYMMETRY_ERROR_DB = 6.0
MAX_BUDGET_EXTENTS = 2
MAX_RAYS_PER_DISTRIBUTED_EXTENT = 16

SUMMARY_MARKER = "[AUDIO][CANOPY][SUMMARY]"
SAMPLE_MARKER = "[AUDIO][CANOPY][SAMPLE]"


@dataclass(frozen=True)
class Summary:
    time_token: str
    time_seconds: float
    elapsed_seconds: float
    phase: str
    values: dict[str, int]


@dataclass(frozen=True)
class Sample:
    time_token: str
    time_seconds: float
    tree: int
    generation: int
    sample_id: int
    emitter: str
    voice: int | None
    position_world: tuple[float, float, float]
    observed_world: tuple[float, float, float] | None
    clearance: float
    weight: float
    observed_weight: float | None
    provenance: str
    membership: bool | None
    status: str | None
    hit: bool | None
    hit_material: str | None
    transmission: tuple[float, float, float] | None
    raw_gain: tuple[float, float, float] | None
    filtered_gain: tuple[float, float, float] | None
    rays: int | None
    spatial_revision: int | None
    geometry_version: int | None
    response_spatial_revision: int | None
    response_geometry_version: int | None


@dataclass(frozen=True)
class DiagnosticMetric:
    mode: str
    accepted: bool
    failures: tuple[str, ...]
    tree_count: int
    emitter_count: int
    voice_count: int
    sample_count: int
    minimum_clearance: float
    total_power: float
    max_adjacent_gain_step_db: float
    max_hold_gain_step_db: float
    max_raw_symmetry_error_db: float
    max_filtered_symmetry_error_db: float
    extent_response_count: int
    processed_extent_count: int
    retained_count: int
    deferred_count: int
    direct_ray_count: int


def _token(line: str, key: str) -> str:
    match = re.search(rf"(?:^|\s){re.escape(key)}=([^\s]+)", line)
    if match is None:
        raise ValueError(f"missing {key} in telemetry line")
    return match.group(1)


def _optional_inner(line: str, key: str) -> str | None:
    match = re.search(rf"(?:^|\s){re.escape(key)}=(None|Some\(([^\s)]+)\))", line)
    if match is None:
        raise ValueError(f"missing {key} in telemetry line")
    return None if match.group(1) == "None" else match.group(2)


def _float_triplet(value: str) -> tuple[float, float, float]:
    parts = [float(part.strip()) for part in value.split(",")]
    if len(parts) != 3 or not all(math.isfinite(part) for part in parts):
        raise ValueError(f"invalid telemetry vector [{value}]")
    return (parts[0], parts[1], parts[2])


def _vec3(line: str, key: str) -> tuple[float, float, float]:
    match = re.search(rf"(?:^|\s){re.escape(key)}=Vec3\(([^)]+)\)", line)
    if match is None:
        raise ValueError(f"missing {key} in sample telemetry")
    return _float_triplet(match.group(1))


def _optional_vec3(line: str, key: str) -> tuple[float, float, float] | None:
    match = re.search(rf"(?:^|\s){re.escape(key)}=(None|Some\(Vec3\(([^)]+)\)\))", line)
    if match is None:
        raise ValueError(f"missing {key} in sample telemetry")
    return None if match.group(1) == "None" else _float_triplet(match.group(2))


def _optional_triplet(line: str, key: str) -> tuple[float, float, float] | None:
    match = re.search(rf"(?:^|\s){re.escape(key)}=(None|Some\(\[([^]]+)\]\))", line)
    if match is None:
        raise ValueError(f"missing {key} in sample telemetry")
    return None if match.group(1) == "None" else _float_triplet(match.group(2))


def _optional_bool(line: str, key: str) -> bool | None:
    value = _optional_inner(line, key)
    if value is None:
        return None
    if value not in {"true", "false"}:
        raise ValueError(f"invalid {key}={value}")
    return value == "true"


def _optional_int(line: str, key: str) -> int | None:
    value = _optional_inner(line, key)
    return None if value is None else int(value)


def _parse_summary(line: str) -> Summary:
    time_token = _token(line, "time_seconds")
    phase = _optional_inner(line, "trajectory_phase")
    if phase is None:
        phase = "Inactive"
    integer_keys = (
        "trees",
        "emitters",
        "observed_voices",
        "runtime_emitters",
        "runtime_voices",
        "samples",
        "extent_responses",
        "solve_discards",
        "voice_identity_violations",
        "revision_rollbacks",
        "sample_contract_violations",
        "aggregate_mismatches",
        "telemetry_queue_depth",
        "telemetry_queue_high_water",
        "telemetry_drops",
        "direct_rays",
        "processed_extents",
        "retained",
        "deferred",
        "render_rejected_rollbacks",
    )
    return Summary(
        time_token=time_token,
        time_seconds=float(time_token),
        elapsed_seconds=float(_token(line, "trajectory_elapsed_seconds")),
        phase=phase,
        values={key: int(_token(line, key)) for key in integer_keys},
    )


def _parse_sample(line: str) -> Sample:
    time_token = _token(line, "time_seconds")
    voice = _optional_int(line, "voice")
    observed_weight = _optional_inner(line, "observed_weight")
    status = _optional_inner(line, "solve_status")
    hit_material_match = re.search(r"(?:^|\s)hit_material=(None|Some\(\"([^\"]+)\"\))", line)
    if hit_material_match is None:
        raise ValueError("missing hit_material in sample telemetry")
    hit_material = (
        None if hit_material_match.group(1) == "None" else hit_material_match.group(2)
    )
    return Sample(
        time_token=time_token,
        time_seconds=float(time_token),
        tree=int(_token(line, "tree")),
        generation=int(_token(line, "generation")),
        sample_id=int(_token(line, "sample")),
        emitter=_token(line, "emitter"),
        voice=voice,
        position_world=_vec3(line, "position_world"),
        observed_world=_optional_vec3(line, "observed_world"),
        clearance=float(_token(line, "clearance_voxels")),
        weight=float(_token(line, "weight")),
        observed_weight=None if observed_weight is None else float(observed_weight),
        provenance=_token(line, "provenance"),
        membership=_optional_bool(line, "candidate_membership"),
        status=status,
        hit=_optional_bool(line, "hit"),
        hit_material=hit_material,
        transmission=_optional_triplet(line, "transmission"),
        raw_gain=_optional_triplet(line, "raw_gain"),
        filtered_gain=_optional_triplet(line, "filtered_gain"),
        rays=_optional_int(line, "rays"),
        spatial_revision=_optional_int(line, "spatial_revision"),
        geometry_version=_optional_int(line, "geometry_version"),
        response_spatial_revision=_optional_int(line, "response_spatial_revision"),
        response_geometry_version=_optional_int(line, "response_geometry_version"),
    )


def _distance(left: tuple[float, float, float], right: tuple[float, float, float]) -> float:
    return math.sqrt(sum((a - b) ** 2 for a, b in zip(left, right)))


def _gain_error_db(
    left: tuple[float, float, float], right: tuple[float, float, float]
) -> float:
    return max(
        abs(20.0 * math.log10(max(a, 1.0e-6) / max(b, 1.0e-6)))
        for a, b in zip(left, right)
    )


def _max_adjacent_error(
    observations: list[tuple[float, tuple[float, float, float]]],
) -> float:
    return max(
        (_gain_error_db(left[1], right[1]) for left, right in zip(observations, observations[1:])),
        default=0.0,
    )


def analyze_text(text: str, expected_mode: str = "auto") -> DiagnosticMetric:
    summaries: list[Summary] = []
    samples: list[Sample] = []
    parse_failures: list[str] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        try:
            if SUMMARY_MARKER in line:
                summaries.append(_parse_summary(line))
            elif SAMPLE_MARKER in line:
                samples.append(_parse_sample(line))
        except (ValueError, OverflowError) as error:
            parse_failures.append(f"line {line_number}: {error}")

    if not summaries:
        raise ValueError("log contains no canopy summary telemetry")
    if not samples:
        raise ValueError("log contains no canopy sample telemetry")

    final = summaries[-1]
    mode = "budget" if final.values["trees"] > 1 else "single"
    if expected_mode not in {"auto", "single", "budget"}:
        raise ValueError(f"invalid expected mode {expected_mode}")
    failures = parse_failures
    if expected_mode != "auto" and mode != expected_mode:
        failures.append(f"expected {expected_mode} diagnostic, observed {mode}")
    if "Application exited successfully" not in text:
        failures.append("application did not exit successfully")

    summary_by_time = {summary.time_token: summary for summary in summaries}
    if any(sample.time_token not in summary_by_time for sample in samples):
        failures.append("sample telemetry has no same-frame summary marker")

    for key in (
        "voice_identity_violations",
        "revision_rollbacks",
        "sample_contract_violations",
        "aggregate_mismatches",
        "telemetry_drops",
        "render_rejected_rollbacks",
    ):
        if final.values[key] != 0:
            failures.append(f"{key}={final.values[key]}, expected 0")
    if final.values["telemetry_queue_depth"] != 0:
        failures.append(
            f"telemetry_queue_depth={final.values['telemetry_queue_depth']}, expected 0"
        )

    grouped: dict[tuple[str, int, int], list[Sample]] = {}
    stable_groups: dict[tuple[int, int], list[list[Sample]]] = {}
    for sample in samples:
        grouped.setdefault((sample.time_token, sample.tree, sample.generation), []).append(sample)
    for (_, tree, generation), group in grouped.items():
        stable_groups.setdefault((tree, generation), []).append(group)

        ids = {sample.sample_id for sample in group}
        if len(ids) != len(group) or not 1 <= len(group) <= 8:
            failures.append(
                f"tree={tree} generation={generation} publishes {len(group)} non-unique/out-of-bound samples"
            )
        total_power = sum(sample.weight for sample in group)
        if abs(total_power - 1.0) > 1.0e-4:
            failures.append(
                f"tree={tree} generation={generation} total_power={total_power:.9f}"
            )
        emitters = {sample.emitter for sample in group}
        voices = {sample.voice for sample in group if sample.voice is not None}
        if len(emitters) != 1 or len(voices) > 1:
            failures.append(
                f"tree={tree} generation={generation} has emitters={len(emitters)} voices={len(voices)}"
            )
        for sample in group:
            if sample.clearance + 1.0e-6 < MIN_WOOD_CLEARANCE_VOXELS:
                failures.append(
                    f"tree={tree} sample={sample.sample_id} clearance={sample.clearance:.6f}"
                )
            if sample.provenance != "LeafPlacement":
                failures.append(
                    f"tree={tree} sample={sample.sample_id} provenance={sample.provenance}"
                )
            if sample.observed_weight is not None and abs(sample.observed_weight - sample.weight) > 1.0e-5:
                failures.append(f"tree={tree} sample={sample.sample_id} weight mismatch")
            if sample.observed_world is not None and _distance(sample.position_world, sample.observed_world) > 1.0e-5:
                failures.append(f"tree={tree} sample={sample.sample_id} position mismatch")
            if sample.spatial_revision is not None and sample.response_spatial_revision is not None:
                if sample.response_spatial_revision > sample.spatial_revision:
                    failures.append(f"tree={tree} sample={sample.sample_id} future response revision")
                elif (
                    sample.status == "Solved"
                    and sample.response_spatial_revision != sample.spatial_revision
                ):
                    failures.append(f"tree={tree} sample={sample.sample_id} stale solved revision")
            if sample.geometry_version is not None and sample.response_geometry_version is not None:
                if sample.response_geometry_version > sample.geometry_version:
                    failures.append(f"tree={tree} sample={sample.sample_id} future geometry version")
                elif (
                    sample.status == "Solved"
                    and sample.response_geometry_version != sample.geometry_version
                ):
                    failures.append(f"tree={tree} sample={sample.sample_id} stale solved geometry")
            if sample.hit is False and sample.transmission != (1.0, 1.0, 1.0):
                failures.append(f"tree={tree} sample={sample.sample_id} unoccluded transmission mismatch")
            if sample.hit is True and (
                sample.hit_material is None
                or sample.transmission is None
                or any(value < 0.0 or value > 1.0 for value in sample.transmission)
            ):
                failures.append(f"tree={tree} sample={sample.sample_id} invalid hit mapping")

    for (tree, generation), snapshots in stable_groups.items():
        baseline = {sample.sample_id: (sample.weight, sample.position_world) for sample in snapshots[0]}
        baseline_emitter = snapshots[0][0].emitter
        for snapshot in snapshots[1:]:
            current = {sample.sample_id: (sample.weight, sample.position_world) for sample in snapshot}
            if current != baseline or snapshot[0].emitter != baseline_emitter:
                failures.append(f"tree={tree} generation={generation} changed immutable extent")
                break

    final_groups = [
        group for (time, _, _), group in grouped.items() if time == final.time_token
    ]
    direct_final_groups = [group for group in final_groups if group[0].voice is not None]
    final_emitters = {group[0].emitter for group in final_groups}
    final_voices = {group[0].voice for group in direct_final_groups}
    if len(final_emitters) != final.values["emitters"]:
        failures.append("summary emitter count does not match sample identities")
    if len(final_voices) != final.values["observed_voices"]:
        failures.append("summary voice count does not match sample identities")
    if final.values["runtime_emitters"] != final.values["emitters"]:
        failures.append("runtime emitter count does not match canopy generations")
    if final.values["runtime_voices"] != final.values["observed_voices"]:
        failures.append("runtime Voice/cursor count does not match canopy generations")
    if sum(len(group) for group in final_groups) != final.values["samples"]:
        failures.append("summary sample count does not match sample rows")

    observations: list[
        tuple[float, tuple[float, float, float], tuple[float, float, float]]
    ] = []
    filtered_by_tree: dict[int, list[tuple[float, tuple[float, float, float]]]] = {}
    for (time, tree, _), group in grouped.items():
        summary = summary_by_time.get(time)
        first = group[0]
        if (
            summary is not None
            and first.raw_gain is not None
            and first.filtered_gain is not None
        ):
            filtered_by_tree.setdefault(tree, []).append(
                (summary.elapsed_seconds, first.filtered_gain)
            )
            if tree == 0:
                observations.append(
                    (summary.elapsed_seconds, first.raw_gain, first.filtered_gain)
                )
    observations.sort(key=lambda value: value[0])
    raw_observations = [(elapsed, raw) for elapsed, raw, _ in observations]
    max_adjacent = _max_adjacent_error(raw_observations)
    hold_observations = [value for value in raw_observations if 5.1 <= value[0] <= 5.9]
    max_hold = _max_adjacent_error(hold_observations)
    max_raw_symmetry = 0.0
    max_filtered_symmetry = 0.0

    if mode == "single":
        if final.values["trees"] != 1 or final.values["emitters"] != 1 or final.values["observed_voices"] != 1:
            failures.append("single diagnostic must end with one tree/emitter/Voice")
        if final.values["samples"] != 8 or len(final_groups) != 1:
            failures.append("single diagnostic must end with exactly eight physical samples")
        phases = {summary.phase for summary in summaries}
        for phase in ("ForwardOrbit", "OcclusionBoundaryHold", "ReverseOrbit"):
            if phase not in phases:
                failures.append(f"trajectory never observed {phase}")
        if max_adjacent > MAX_ADJACENT_GAIN_STEP_DB:
            failures.append(f"adjacent raw gain step {max_adjacent:.3f} dB exceeds bound")
        if len(hold_observations) < 2:
            failures.append("hold segment has insufficient aggregate observations")
        elif max_hold > MAX_HOLD_GAIN_STEP_DB:
            failures.append(f"hold raw gain step {max_hold:.3f} dB exceeds hold bound")

        forward = [value for value in observations if 1.25 <= value[0] <= 4.75]
        reverse = [value for value in observations if 6.25 <= value[0] <= 9.75]
        pair_count = 0
        for elapsed, raw, filtered in forward:
            if not reverse:
                continue
            paired = min(reverse, key=lambda value: abs(value[0] - (11.0 - elapsed)))
            if abs(paired[0] - (11.0 - elapsed)) > 0.16:
                continue
            pair_count += 1
            max_raw_symmetry = max(max_raw_symmetry, _gain_error_db(raw, paired[1]))
            max_filtered_symmetry = max(
                max_filtered_symmetry, _gain_error_db(filtered, paired[2])
            )
        if pair_count < 3:
            failures.append("forward/reverse trajectory has insufficient matched positions")
        if max_raw_symmetry > MAX_RAW_SYMMETRY_ERROR_DB:
            failures.append(f"raw forward/reverse error {max_raw_symmetry:.3f} dB")
        if max_filtered_symmetry > MAX_FILTERED_SYMMETRY_ERROR_DB:
            failures.append(f"filtered forward/reverse error {max_filtered_symmetry:.3f} dB")
    else:
        for tree_observations in filtered_by_tree.values():
            tree_observations.sort(key=lambda value: value[0])
        max_adjacent = max(
            (_max_adjacent_error(values) for values in filtered_by_tree.values()),
            default=0.0,
        )
        max_hold = max(
            (
                _max_adjacent_error(
                    [value for value in values if 5.1 <= value[0] <= 5.9]
                )
                for values in filtered_by_tree.values()
            ),
            default=0.0,
        )
        if max_adjacent > MAX_ADJACENT_GAIN_STEP_DB:
            failures.append(f"budget filtered gain step {max_adjacent:.3f} dB exceeds bound")
        if max_hold > MAX_HOLD_GAIN_STEP_DB:
            failures.append(f"budget hold filtered gain step {max_hold:.3f} dB exceeds bound")
        if final.values["trees"] != 5 or final.values["emitters"] != 5 or final.values["observed_voices"] != 5:
            failures.append("budget diagnostic must end with five tree/emitter/Voice identities")
        solved_per_frame: dict[str, set[tuple[int, int]]] = {}
        retained_observed = False
        deferred_non_unity_observed = False
        for (time, tree, generation), group in grouped.items():
            status = group[0].status
            if status == "Solved":
                solved_per_frame.setdefault(time, set()).add((tree, generation))
            elif status == "Retained":
                retained_observed = True
            elif status == "Deferred":
                deferred_non_unity_observed |= (
                    group[0].rays == 0
                    and group[0].raw_gain is not None
                    and any(abs(value - 1.0) > 1.0e-3 for value in group[0].raw_gain)
                )
        if any(len(solved) > MAX_BUDGET_EXTENTS for solved in solved_per_frame.values()):
            failures.append("more than two extents were solved in one budget frame")
        if final.values["retained"] <= 0 or not retained_observed:
            failures.append("budget diagnostic did not expose Retained last-good response")
        if final.values["deferred"] <= 0 or not deferred_non_unity_observed:
            failures.append("budget diagnostic did not expose bounded non-unity Deferred response")
        partition = (
            final.values["processed_extents"]
            + final.values["retained"]
            + final.values["deferred"]
        )
        if partition != final.values["extent_responses"]:
            failures.append("budget solve-status counts do not partition extent responses")
        max_rays = (
            final.values["processed_extents"] * MAX_RAYS_PER_DISTRIBUTED_EXTENT
        )
        if final.values["direct_rays"] > max_rays:
            failures.append("direct ray count exceeded weighted-sample extent budget")

    minimum_clearance = min(sample.clearance for sample in samples)
    final_total_power = sum(sample.weight for group in final_groups for sample in group)
    return DiagnosticMetric(
        mode=mode,
        accepted=not failures,
        failures=tuple(dict.fromkeys(failures)),
        tree_count=final.values["trees"],
        emitter_count=final.values["emitters"],
        voice_count=final.values["observed_voices"],
        sample_count=final.values["samples"],
        minimum_clearance=minimum_clearance,
        total_power=final_total_power / max(len(final_groups), 1),
        max_adjacent_gain_step_db=max_adjacent,
        max_hold_gain_step_db=max_hold,
        max_raw_symmetry_error_db=max_raw_symmetry,
        max_filtered_symmetry_error_db=max_filtered_symmetry,
        extent_response_count=final.values["extent_responses"],
        processed_extent_count=final.values["processed_extents"],
        retained_count=final.values["retained"],
        deferred_count=final.values["deferred"],
        direct_ray_count=final.values["direct_rays"],
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path)
    parser.add_argument("--expect", choices=("auto", "single", "budget"), default="auto")
    args = parser.parse_args()
    try:
        metric = analyze_text(
            args.log.read_text(encoding="utf-8", errors="replace"), args.expect
        )
    except (OSError, ValueError) as error:
        print(f"[CANOPY_AUDIO_ACCEPTANCE] verdict=ERROR reason={error}", file=sys.stderr)
        return 2

    verdict = "PASS" if metric.accepted else "FAIL"
    print(
        f"[CANOPY_AUDIO_ACCEPTANCE] verdict={verdict} mode={metric.mode} "
        f"trees={metric.tree_count} emitters={metric.emitter_count} voices={metric.voice_count} "
        f"samples={metric.sample_count} total_power={metric.total_power:.9f} "
        f"min_clearance_voxels={metric.minimum_clearance:.6f} "
        f"step_domain={'raw' if metric.mode == 'single' else 'filtered'} "
        f"max_step_db={metric.max_adjacent_gain_step_db:.3f} "
        f"hold_step_db={metric.max_hold_gain_step_db:.3f} "
        f"raw_symmetry_db={metric.max_raw_symmetry_error_db:.3f} "
        f"filtered_symmetry_db={metric.max_filtered_symmetry_error_db:.3f} "
        f"extent_responses={metric.extent_response_count} "
        f"processed={metric.processed_extent_count} retained={metric.retained_count} "
        f"deferred={metric.deferred_count} rays={metric.direct_ray_count}"
    )
    for failure in metric.failures:
        print(f"[CANOPY_AUDIO_ACCEPTANCE] failure={failure}", file=sys.stderr)
    return 0 if metric.accepted else 1


if __name__ == "__main__":
    raise SystemExit(main())
