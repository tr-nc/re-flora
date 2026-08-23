#!/usr/bin/env python3
"""Run and summarize the deterministic detached-terrain release benchmark."""

from __future__ import annotations

import argparse
import fcntl
import hashlib
import json
import math
import platform
import re
import subprocess
import time
from collections.abc import Iterable
from pathlib import Path
from typing import Any


MARKER = "[PERF][TERRAIN_CONNECTIVITY_BENCH]"
FIELD_RE = re.compile(r"\b([a-z_]+)=([^\s]+)")
RUN_LOG_RE = re.compile(r"Writing run log to (.+)$", re.MULTILINE)
PERF_LOCK = Path("/tmp/re-flora-authoritative-perf.lock")
NUMERIC_FIELDS = {
    "frame",
    "release_frame",
    "relative",
    "available_particles",
    "fixture_voxels",
    "snapshot_voxels",
    "snapshot_readback_us",
    "voxel_budget",
    "processed",
    "processed_total",
    "pending",
    "step_us",
    "total_us",
    "current_path_us",
    "primary_readback_us",
    "trace_readback_us",
    "classification_us",
    "atomic_validation_us",
    "validation_us",
    "sampling_us",
    "invalidation_us",
    "publication_us",
    "particle_spawn_us",
    "classified_voxels",
    "trace_readback_tiles",
    "invalidated_voxels",
    "sampled_voxels",
    "spawned_particles",
    "revision_before",
    "revision_after",
    "release_to_commit_frames",
    "cpu_total_us",
    "gpu_present_us",
    "tracked_us",
    "untracked_us",
    "terrain_collider_pending",
    "contree_cache_pending",
    "water_source_pending",
    "water_collider_pending",
    "water_cache_pending",
    "visible_revision",
    "frame_render_us",
    "tracer_render_us",
    "scopes",
    "dropped",
    "observed_frames",
    "remaining_fixture_voxels",
    "high_water_terrain_collider",
    "high_water_contree_cache",
    "high_water_water_source",
    "high_water_water_collider",
    "high_water_water_cache",
}


def parse_marker_lines(text: str) -> dict[str, list[dict[str, Any]]]:
    phases: dict[str, list[dict[str, Any]]] = {}
    for line in text.splitlines():
        if MARKER not in line:
            continue
        fields: dict[str, Any] = {}
        for key, raw in FIELD_RE.findall(line):
            raw = raw.rstrip(",")
            if key in NUMERIC_FIELDS:
                fields[key] = int(raw) if re.fullmatch(r"-?\d+", raw) else float(raw)
            elif raw in {"true", "false"}:
                fields[key] = raw == "true"
            else:
                fields[key] = raw
        phase = fields.get("phase")
        if phase:
            phases.setdefault(str(phase), []).append(fields)
    return phases


def percentile(values: Iterable[float], fraction: float) -> float:
    ordered = sorted(float(value) for value in values)
    if not ordered:
        raise ValueError("cannot summarize an empty sample")
    rank = (len(ordered) - 1) * fraction
    lower = math.floor(rank)
    upper = math.ceil(rank)
    if lower == upper:
        return ordered[lower]
    weight = rank - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def distribution(values: Iterable[float]) -> dict[str, float | int]:
    samples = [float(value) for value in values]
    return {
        "samples": len(samples),
        "p50_us": round(percentile(samples, 0.50), 3),
        "p95_us": round(percentile(samples, 0.95), 3),
        "p99_us": round(percentile(samples, 0.99), 3),
        "max_us": round(max(samples), 3),
    }


def frame_distribution(values: Iterable[float]) -> dict[str, float | int]:
    summary = distribution(values)
    return {
        "samples": summary["samples"],
        "p50_frames": summary["p50_us"],
        "p95_frames": summary["p95_us"],
        "p99_frames": summary["p99_us"],
        "max_frames": summary["max_us"],
    }


def queue_completion_frame(frames: list[dict[str, Any]], field: str) -> int | None:
    nonzero = [int(frame["relative"]) for frame in frames if int(frame[field]) > 0]
    if not nonzero:
        return 0
    last_nonzero = max(nonzero)
    if any(int(frame["relative"]) > last_nonzero for frame in frames):
        return last_nonzero + 1
    return None


def summarize_run(
    text: str, mode: str, capacity: int, voxel_budget: int | None = None
) -> dict[str, Any]:
    phases = parse_marker_lines(text)
    event = phases.get("event", [])
    summary = phases.get("summary", [])
    if len(event) != 1 or len(summary) != 1:
        raise ValueError(f"expected one event and summary, got {len(event)} and {len(summary)}")
    event_record = event[0]
    summary_record = summary[0]
    if event_record.get("mode") != mode or event_record.get("available_particles") != capacity:
        raise ValueError("event mode/capacity does not match requested case")
    if voxel_budget is not None and event_record.get("voxel_budget") != voxel_budget:
        raise ValueError("event voxel budget does not match requested case")
    event_record["classification_cpu_us"] = float(event_record["classification_us"]) - float(
        event_record.get("trace_readback_us", 0.0)
    )

    cpu_frames = phases.get("frame", [])
    gpu_frames = phases.get("gpu_frame", [])
    pre_cpu = [frame for frame in cpu_frames if int(frame["relative"]) < 0]
    event_cpu = [frame for frame in cpu_frames if int(frame["relative"]) == 0]
    post_cpu = [frame for frame in cpu_frames if int(frame["relative"]) > 0]
    pre_gpu = [frame for frame in gpu_frames if int(frame["relative"]) < 0]
    event_gpu = [frame for frame in gpu_frames if int(frame["relative"]) == 0]
    post_gpu = [frame for frame in gpu_frames if int(frame["relative"]) > 0]
    if not pre_cpu or len(event_cpu) != 1 or not post_cpu or not pre_gpu or not event_gpu or not post_gpu:
        raise ValueError("missing pre/event/post CPU or GPU profiler samples")

    revision_before = int(event_record["revision_before"])
    revision_after = int(event_record["revision_after"])
    expected_remaining = 437_205 if mode == "existing" else 0
    expected_after = revision_before if mode == "existing" else revision_before + 1
    atomic_visibility = {
        "pre_frames_old_revision": all(
            int(frame["visible_revision"]) == revision_before for frame in pre_cpu
        ),
        "event_and_post_frames_final_revision": all(
            int(frame["visible_revision"]) == expected_after for frame in event_cpu + post_cpu
        ),
        "final_fixture_voxels": int(summary_record["remaining_fixture_voxels"]),
        "expected_fixture_voxels": expected_remaining,
        "no_observed_partial_revision": True,
    }
    atomic_checks_pass = (
        atomic_visibility["pre_frames_old_revision"]
        and atomic_visibility["event_and_post_frames_final_revision"]
        and atomic_visibility["no_observed_partial_revision"]
        and atomic_visibility["final_fixture_voxels"]
        == atomic_visibility["expected_fixture_voxels"]
    )
    if revision_after != expected_after or not atomic_checks_pass:
        raise ValueError(f"atomic terrain visibility invariant failed: {atomic_visibility}")

    queue_fields = [
        "terrain_collider_pending",
        "contree_cache_pending",
        "water_source_pending",
        "water_collider_pending",
        "water_cache_pending",
    ]
    queue_frames = event_cpu + post_cpu
    result = {
        "mode": mode,
        "available_particles": capacity,
        "event": event_record,
        "summary": summary_record,
        "frame_cpu": {
            "pre": distribution(frame["cpu_total_us"] for frame in pre_cpu),
            "event_us": float(event_cpu[0]["cpu_total_us"]),
            "post": distribution(frame["cpu_total_us"] for frame in post_cpu),
        },
        "frame_gpu": {
            "pre": distribution(frame["frame_render_us"] for frame in pre_gpu),
            "event_us": float(event_gpu[0]["frame_render_us"]),
            "post": distribution(frame["frame_render_us"] for frame in post_gpu),
        },
        "present_cpu": {
            "pre": distribution(frame["gpu_present_us"] for frame in pre_cpu),
            "event_us": float(event_cpu[0]["gpu_present_us"]),
            "post": distribution(frame["gpu_present_us"] for frame in post_cpu),
        },
        "queues": {
            field: {
                "high_water": max(int(frame[field]) for frame in queue_frames),
                "drained_by_relative_frame": queue_completion_frame(queue_frames, field),
            }
            for field in queue_fields
        }
        | {
            "ddgi_ready_by_relative_frame": next(
                (
                    int(frame["relative"])
                    for frame in queue_frames
                    if bool(frame["ddgi_ready"])
                ),
                None,
            )
        },
        "atomic_visibility": atomic_visibility,
        "samples": {
            "pre_cpu": len(pre_cpu),
            "post_cpu": len(post_cpu),
            "pre_gpu": len(pre_gpu),
            "post_gpu": len(post_gpu),
        },
    }
    if mode == "bounded":
        job_frames = phases.get("job_frame", [])
        atomic_checks = phases.get("atomic_check", [])
        if (
            not job_frames
            or len(atomic_checks) != 1
            or job_frames[-1].get("disposition") != "detached"
            or any(frame.get("disposition") != "pending" for frame in job_frames[:-1])
            or int(job_frames[-1]["processed_total"]) != 437_205
            or int(atomic_checks[0]["remaining_fixture_voxels"]) != 437_205
            or int(atomic_checks[0]["visible_revision"]) != revision_before
        ):
            raise ValueError("bounded job Pending/Detached or atomic pre-commit invariant failed")
        result["bounded_job"] = {
            "steps": len(job_frames),
            "completion_relative_frame": int(job_frames[-1]["relative"]),
            "processed_voxels": int(job_frames[-1]["processed_total"]),
            "step_cpu": distribution(frame["step_us"] for frame in job_frames),
            "classification_cpu_total_us": float(job_frames[-1]["classification_us"]),
            "max_pending": max(int(frame["pending"]) for frame in job_frames),
            "atomic_validation_us": float(atomic_checks[0]["validation_us"]),
            "atomic_check_frame": int(atomic_checks[0]["frame"]),
        }
    return result


def aggregate_case(runs: list[dict[str, Any]]) -> dict[str, Any]:
    stage_names = [
        "total_us",
        "current_path_us",
        "primary_readback_us",
        "trace_readback_us",
        "classification_us",
        "classification_cpu_us",
        "atomic_validation_us",
        "sampling_us",
        "invalidation_us",
        "publication_us",
        "particle_spawn_us",
    ]
    result = {
        "runs": len(runs),
        "stages": {
            stage: distribution(run["event"][stage] for run in runs) for stage in stage_names
        },
        "event_frame_cpu": distribution(run["frame_cpu"]["event_us"] for run in runs),
        "event_frame_gpu": distribution(run["frame_gpu"]["event_us"] for run in runs),
        "pre_frame_cpu": distribution(
            value
            for run in runs
            for value in _phase_samples(run, "frame", "cpu", "pre")
        ),
        "post_frame_cpu": distribution(
            value
            for run in runs
            for value in _phase_samples(run, "frame", "cpu", "post")
        ),
        "pre_frame_gpu": distribution(
            value
            for run in runs
            for value in _phase_samples(run, "frame", "gpu", "pre")
        ),
        "post_frame_gpu": distribution(
            value
            for run in runs
            for value in _phase_samples(run, "frame", "gpu", "post")
        ),
        "queue_high_water": {
            field: max(run["queues"][field]["high_water"] for run in runs)
            for field in (
                "terrain_collider_pending",
                "contree_cache_pending",
                "water_source_pending",
                "water_collider_pending",
                "water_cache_pending",
            )
        },
        "queue_drain_relative_frame_max": {
            field: _max_optional(
                run["queues"][field]["drained_by_relative_frame"] for run in runs
            )
            for field in (
                "terrain_collider_pending",
                "contree_cache_pending",
                "water_source_pending",
                "water_collider_pending",
                "water_cache_pending",
            )
        }
        | {
            "ddgi_ready": _max_optional(
                run["queues"]["ddgi_ready_by_relative_frame"] for run in runs
            )
        },
        "all_atomic_visibility_checks_passed": all(
            run["atomic_visibility"]["pre_frames_old_revision"]
            and run["atomic_visibility"]["event_and_post_frames_final_revision"]
            and run["atomic_visibility"]["no_observed_partial_revision"]
            and run["atomic_visibility"]["final_fixture_voxels"]
            == run["atomic_visibility"]["expected_fixture_voxels"]
            for run in runs
        ),
    }
    if "bounded_job" in runs[0]:
        result["bounded_job"] = {
            "completion_relative_frame": frame_distribution(
                run["bounded_job"]["completion_relative_frame"] for run in runs
            ),
            "step_cpu": distribution(
                step
                for run in runs
                for step in run["bounded_job"]["_step_cpu_samples"]
            ),
            "classification_cpu_total": distribution(
                run["bounded_job"]["classification_cpu_total_us"] for run in runs
            ),
            "atomic_validation": distribution(
                run["bounded_job"]["atomic_validation_us"] for run in runs
            ),
            "max_pending": max(run["bounded_job"]["max_pending"] for run in runs),
        }
    return result


def _phase_samples(run: dict[str, Any], prefix: str, processor: str, phase: str) -> list[float]:
    # Per-run percentiles cannot be pooled back into raw samples. The runner stores parsed frames
    # privately until aggregation and replaces this helper before emitting JSON.
    return run[f"_{prefix}_{processor}_{phase}_samples"]


def _max_optional(values: Iterable[int | None]) -> int | None:
    values = list(values)
    return None if any(value is None for value in values) else max(int(value) for value in values)


def enrich_private_samples(run: dict[str, Any], text: str) -> None:
    phases = parse_marker_lines(text)
    for processor, phase_name, metric in (
        ("cpu", "frame", "cpu_total_us"),
        ("gpu", "gpu_frame", "frame_render_us"),
    ):
        frames = phases.get(phase_name, [])
        for phase, predicate in (
            ("pre", lambda relative: relative < 0),
            ("post", lambda relative: relative > 0),
        ):
            run[f"_frame_{processor}_{phase}_samples"] = [
                float(frame[metric])
                for frame in frames
                if predicate(int(frame["relative"]))
            ]
    if "bounded_job" in run:
        run["bounded_job"]["_step_cpu_samples"] = [
            float(frame["step_us"])
            for frame in phases.get("job_frame", [])
        ]


def strip_private_samples(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: strip_private_samples(child)
            for key, child in value.items()
            if not key.startswith("_")
        }
    if isinstance(value, list):
        return [strip_private_samples(child) for child in value]
    return value


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_case(
    binary: Path,
    output_dir: Path,
    mode: str,
    capacity: int,
    repetition: int,
    warmup_frames: int,
    observe_frames: int,
    auto_exit: float,
    voxel_budget: int,
) -> tuple[dict[str, Any], str]:
    command = [
        str(binary),
        "--hidden",
        "--mute",
        "--perf",
        "--camera-snapshot",
        "player-default",
        "--auto-exit",
        str(auto_exit),
        "--terrain-connectivity-bench",
        mode,
        "--terrain-connectivity-bench-available-particles",
        str(capacity),
        "--terrain-connectivity-bench-warmup-frames",
        str(warmup_frames),
        "--terrain-connectivity-bench-observe-frames",
        str(observe_frames),
        "--terrain-connectivity-bench-voxel-budget",
        str(voxel_budget),
    ]
    started = time.monotonic()
    completed = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    elapsed = time.monotonic() - started
    raw_path = output_dir / f"{repetition:02d}-{mode}-{capacity}-budget-{voxel_budget}.log"
    raw_path.write_text(completed.stdout, encoding="utf-8")
    if completed.returncode != 0:
        raise RuntimeError(f"benchmark failed ({completed.returncode}); see {raw_path}")
    for forbidden in ("ERROR", "panicked at", "VUID-", "device lost"):
        if forbidden in completed.stdout:
            raise RuntimeError(f"benchmark log contains {forbidden!r}; see {raw_path}")
    run = summarize_run(completed.stdout, mode, capacity, voxel_budget)
    enrich_private_samples(run, completed.stdout)
    log_match = RUN_LOG_RE.search(completed.stdout)
    run.update(
        {
            "repetition": repetition,
            "voxel_budget": voxel_budget,
            "elapsed_seconds": round(elapsed, 3),
            "command": command,
            "raw_log": str(raw_path.resolve()),
            "app_log": log_match.group(1) if log_match else None,
            "runtime": runtime_provenance(completed.stdout),
        }
    )
    return run, completed.stdout


def runtime_provenance(text: str) -> dict[str, str]:
    patterns = {
        "gpu": r"Selected physical device: (.+)$",
        "physical_extent": r"Hidden window render extent is (.+)$",
        "present_mode": r"Chosen swapchain present mode: (.+)$",
        "swapchain_images": r"Swapchain image count: (.+)$",
    }
    values = {
        key: match.group(1)
        for key, pattern in patterns.items()
        if (match := re.search(pattern, text, re.MULTILINE))
    }
    if values.keys() != patterns.keys():
        raise ValueError(f"missing fixed runtime provenance: {values}")
    return values


def baseline_run_order(repetitions: int) -> list[tuple[int, str, int, int]]:
    capacities = [16_384, 8_192, 0]
    order: list[tuple[int, str, int, int]] = []
    for repetition in range(1, repetitions + 1):
        rotated = capacities[(repetition - 1) % len(capacities) :] + capacities[
            : (repetition - 1) % len(capacities)
        ]
        modes = ("existing", "correct") if repetition % 2 else ("correct", "existing")
        for capacity in rotated:
            for mode in modes:
                order.append((repetition, mode, capacity, 16_384))
    return order


def bounded_run_order(
    repetitions: int, cases: list[tuple[int, int]] | None = None
) -> list[tuple[int, str, int, int]]:
    cases = cases or [
        (16_384, 8_192),
        (16_384, 16_384),
        (16_384, 32_768),
        (16_384, 65_536),
        (8_192, 16_384),
        (0, 16_384),
    ]
    order: list[tuple[int, str, int, int]] = []
    for repetition in range(1, repetitions + 1):
        rotated = cases[(repetition - 1) % len(cases) :] + cases[: (repetition - 1) % len(cases)]
        if repetition % 2 == 0:
            rotated.reverse()
        order.extend(
            (repetition, "bounded", capacity, budget) for capacity, budget in rotated
        )
    return order


def parse_bounded_cases(values: list[str] | None) -> list[tuple[int, int]] | None:
    if not values:
        return None
    cases = []
    for value in values:
        try:
            capacity_text, budget_text = value.split(":", 1)
            capacity = int(capacity_text)
            budget = int(budget_text)
        except ValueError as error:
            raise argparse.ArgumentTypeError(
                f"invalid bounded case {value!r}; expected CAPACITY:VOXEL_BUDGET"
            ) from error
        if capacity not in {0, 8_192, 16_384} or budget < 1:
            raise argparse.ArgumentTypeError(
                "bounded capacity must be 0, 8192, or 16384 and budget must be positive"
            )
        cases.append((capacity, budget))
    return cases


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--warmup-frames", type=int, default=600)
    parser.add_argument("--observe-frames", type=int, default=180)
    parser.add_argument("--auto-exit", type=float, default=90.0)
    parser.add_argument("--suite", choices=("baseline", "bounded"), default="baseline")
    parser.add_argument(
        "--bounded-case",
        action="append",
        help="bounded case as CAPACITY:VOXEL_BUDGET; repeat to select multiple cases",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        bounded_cases = parse_bounded_cases(args.bounded_case)
    except argparse.ArgumentTypeError as error:
        raise SystemExit(str(error)) from error
    if bounded_cases is not None and args.suite != "bounded":
        raise SystemExit("--bounded-case requires --suite bounded")
    binary = args.binary.resolve()
    if not binary.is_file():
        raise SystemExit(f"release binary does not exist: {binary}")
    if args.runs < 1 or args.warmup_frames < 0 or args.observe_frames < 1:
        raise SystemExit("runs/observe must be positive and warmup non-negative")
    args.output.mkdir(parents=True, exist_ok=True)

    lock_started = time.monotonic()
    with PERF_LOCK.open("a+") as lock_file:
        fcntl.flock(lock_file, fcntl.LOCK_EX)
        lock_wait = time.monotonic() - lock_started
        cases: dict[str, list[dict[str, Any]]] = {}
        provenance_text = ""
        fixed_runtime: dict[str, str] | None = None
        order = (
            baseline_run_order(args.runs)
            if args.suite == "baseline"
            else bounded_run_order(args.runs, bounded_cases)
        )
        for index, (repetition, mode, capacity, voxel_budget) in enumerate(order, 1):
            print(
                f"[{index}/{len(order)}] repetition={repetition} mode={mode} capacity={capacity} voxel_budget={voxel_budget}",
                flush=True,
            )
            run, text = run_case(
                binary,
                args.output,
                mode,
                capacity,
                repetition,
                args.warmup_frames,
                args.observe_frames,
                args.auto_exit,
                voxel_budget,
            )
            case_name = (
                f"{mode}-{capacity}"
                if mode != "bounded"
                else f"bounded-{capacity}-budget-{voxel_budget}"
            )
            cases.setdefault(case_name, []).append(run)
            if fixed_runtime is None:
                fixed_runtime = run["runtime"]
            elif run["runtime"] != fixed_runtime:
                raise RuntimeError(
                    f"runtime path drifted: expected {fixed_runtime}, got {run['runtime']}"
                )
            provenance_text = text

    assert fixed_runtime is not None
    report = {
        "schema": "terrain-connectivity-perf-v1",
        "provenance": {
            "git_head": subprocess.check_output(
                ["git", "rev-parse", "HEAD"], text=True
            ).strip(),
            "git_status_short": subprocess.check_output(
                ["git", "status", "--short"], text=True
            ).splitlines(),
            "binary": str(binary),
            "binary_sha256": sha256(binary),
            "hostname": platform.node(),
            "platform": platform.platform(),
            **fixed_runtime,
            "camera_snapshot": "player-default",
            "perf_lock": str(PERF_LOCK),
            "perf_lock_wait_seconds": round(lock_wait, 6),
            "runs_per_case": args.runs,
            "warmup_frames": args.warmup_frames,
            "observe_frames": args.observe_frames,
            "run_order": [
                {
                    "repetition": repetition,
                    "mode": mode,
                    "capacity": capacity,
                    "voxel_budget": voxel_budget,
                }
                for repetition, mode, capacity, voxel_budget in order
            ],
            "suite": args.suite,
        },
        "cases": {
            name: {
                "aggregate": aggregate_case(runs),
                "runs": runs,
            }
            for name, runs in sorted(cases.items())
        },
    }
    report = strip_private_samples(report)
    report_path = args.output / "report.json"
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(report_path.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
