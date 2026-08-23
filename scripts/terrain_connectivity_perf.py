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
    "relative",
    "available_particles",
    "fixture_voxels",
    "total_us",
    "current_path_us",
    "primary_readback_us",
    "trace_readback_us",
    "classification_us",
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


def queue_completion_frame(frames: list[dict[str, Any]], field: str) -> int | None:
    nonzero = [int(frame["relative"]) for frame in frames if int(frame[field]) > 0]
    if not nonzero:
        return 0
    last_nonzero = max(nonzero)
    if any(int(frame["relative"]) > last_nonzero for frame in frames):
        return last_nonzero + 1
    return None


def summarize_run(text: str, mode: str, capacity: int) -> dict[str, Any]:
    phases = parse_marker_lines(text)
    event = phases.get("event", [])
    summary = phases.get("summary", [])
    if len(event) != 1 or len(summary) != 1:
        raise ValueError(f"expected one event and summary, got {len(event)} and {len(summary)}")
    event_record = event[0]
    summary_record = summary[0]
    if event_record.get("mode") != mode or event_record.get("available_particles") != capacity:
        raise ValueError("event mode/capacity does not match requested case")
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
    return {
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


def aggregate_case(runs: list[dict[str, Any]]) -> dict[str, Any]:
    stage_names = [
        "total_us",
        "current_path_us",
        "primary_readback_us",
        "trace_readback_us",
        "classification_us",
        "classification_cpu_us",
        "sampling_us",
        "invalidation_us",
        "publication_us",
        "particle_spawn_us",
    ]
    return {
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
    ]
    started = time.monotonic()
    completed = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    elapsed = time.monotonic() - started
    raw_path = output_dir / f"{repetition:02d}-{mode}-{capacity}.log"
    raw_path.write_text(completed.stdout, encoding="utf-8")
    if completed.returncode != 0:
        raise RuntimeError(f"benchmark failed ({completed.returncode}); see {raw_path}")
    for forbidden in ("ERROR", "panicked at", "VUID-", "device lost"):
        if forbidden in completed.stdout:
            raise RuntimeError(f"benchmark log contains {forbidden!r}; see {raw_path}")
    run = summarize_run(completed.stdout, mode, capacity)
    enrich_private_samples(run, completed.stdout)
    log_match = RUN_LOG_RE.search(completed.stdout)
    run.update(
        {
            "repetition": repetition,
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


def run_order(repetitions: int, capacities: list[int]) -> list[tuple[int, str, int]]:
    order: list[tuple[int, str, int]] = []
    for repetition in range(1, repetitions + 1):
        rotated = capacities[(repetition - 1) % len(capacities) :] + capacities[
            : (repetition - 1) % len(capacities)
        ]
        modes = ("existing", "correct") if repetition % 2 else ("correct", "existing")
        for capacity in rotated:
            for mode in modes:
                order.append((repetition, mode, capacity))
    return order


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--warmup-frames", type=int, default=600)
    parser.add_argument("--observe-frames", type=int, default=180)
    parser.add_argument("--auto-exit", type=float, default=90.0)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
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
        order = run_order(args.runs, [16_384, 8_192, 0])
        for index, (repetition, mode, capacity) in enumerate(order, 1):
            print(
                f"[{index}/{len(order)}] repetition={repetition} mode={mode} capacity={capacity}",
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
            )
            cases.setdefault(f"{mode}-{capacity}", []).append(run)
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
                {"repetition": repetition, "mode": mode, "capacity": capacity}
                for repetition, mode, capacity in order
            ],
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
