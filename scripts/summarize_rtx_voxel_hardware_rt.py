#!/usr/bin/env python3
"""Recompute the RTX voxel experiment summary from committed raw artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import platform
import re
import statistics
import subprocess
import tomllib
from pathlib import Path
from typing import Any


FRAME_PATTERN = re.compile(r"\[PERF\]\[GPU_FRAME_SCOPE\] frame (\d+) .*? (.+)$")
SCOPE_PATTERN = re.compile(r"([A-Za-z0-9_.]+)=(\d+)us")
FRAME_SCOPES = ("frame.render", "tracer.render", "tracer.pass", "tracer.shadow_prepass")
EXPECTED_DENSITY_PERCENT = (5, 25, 75)
EXPECTED_MACRO_DIMENSIONS = (2, 4, 8)
EXPECTED_PHASES = ("initial", "after_edit")
EXPECTED_LOCAL_SAMPLE_ORDER = ("software", "hardware", "hardware", "software")
EXPECTED_FRAME_RUN_ORDER = ("A1", "B1", "B2", "A2")
CORRECTNESS_COUNT_FIELDS = (
    "false_positive_count",
    "false_negative_count",
    "wrong_voxel_count",
    "hit_t_mismatch_count",
)


def percentile(values: list[float], percentile_value: float) -> float:
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    position = (len(ordered) - 1) * percentile_value
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def stats(values: list[float]) -> dict[str, float | int]:
    if not values:
        raise ValueError("cannot summarize an empty sample")
    return {
        "count": len(values),
        "mean": statistics.fmean(values),
        "median": statistics.median(values),
        "p95": percentile(values, 0.95),
        "min": min(values),
        "max": max(values),
    }


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_frame_log(path: Path, tail_samples: int) -> dict[str, Any]:
    frames: list[dict[str, Any]] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        match = FRAME_PATTERN.search(line)
        if not match:
            continue
        scopes = {name: int(value) / 1000.0 for name, value in SCOPE_PATTERN.findall(match.group(2))}
        if all(scope in scopes for scope in FRAME_SCOPES):
            frames.append({"frame": int(match.group(1)), **{scope: scopes[scope] for scope in FRAME_SCOPES}})
    if len(frames) < tail_samples:
        raise ValueError(f"{path}: expected at least {tail_samples} complete frame samples, got {len(frames)}")
    selected = frames[-tail_samples:]
    return {
        "path": str(path),
        "sha256": sha256(path),
        "available_sample_count": len(frames),
        "selected_sample_count": len(selected),
        "selection": f"last {tail_samples} complete GPU_FRAME_SCOPE records",
        "first_selected_frame": selected[0]["frame"],
        "last_selected_frame": selected[-1]["frame"],
        "scopes_ms": {scope: stats([frame[scope] for frame in selected]) for scope in FRAME_SCOPES},
        "selected_samples": selected,
    }


def validate_ray_query_artifact(path: Path, artifact: dict[str, Any]) -> None:
    workload = artifact["workload"]
    if (
        tuple(workload["density_percent"]) != EXPECTED_DENSITY_PERCENT
        or tuple(workload["macro_dimensions"]) != EXPECTED_MACRO_DIMENSIONS
        or tuple(workload["sample_order"]) != EXPECTED_LOCAL_SAMPLE_ORDER
    ):
        raise ValueError(f"{path}: workload does not match the fixed report matrix")

    expected_configurations = {
        (density, macro)
        for density in EXPECTED_DENSITY_PERCENT
        for macro in EXPECTED_MACRO_DIMENSIONS
    }
    configurations = artifact["configurations"]
    actual_configurations = [
        (configuration["requested_density_percent"], configuration["macro_dimension"])
        for configuration in configurations
    ]
    if (
        len(actual_configurations) != len(expected_configurations)
        or set(actual_configurations) != expected_configurations
    ):
        raise ValueError(
            f"{path}: workload Cartesian product is missing, duplicated, or unexpected: "
            f"{actual_configurations}"
        )

    expected_samples = list(enumerate(EXPECTED_LOCAL_SAMPLE_ORDER))
    for configuration in configurations:
        configuration_key = (
            configuration["requested_density_percent"],
            configuration["macro_dimension"],
        )
        for phase_name in EXPECTED_PHASES:
            if phase_name not in configuration:
                raise ValueError(f"{path}: missing phase {configuration_key}/{phase_name}")
            samples = configuration[phase_name].get("samples", [])
            actual_samples = [(sample.get("order_index"), sample.get("mode")) for sample in samples]
            if actual_samples != expected_samples:
                raise ValueError(
                    f"{path}: phase sample sequence differs for "
                    f"{configuration_key}/{phase_name}: {actual_samples}"
                )
            for sample in samples:
                zero_counts = {
                    "traversal_exhausted_count": sample["traversal_exhausted_count"],
                    "query_committed_disagreement_count": sample[
                        "query_committed_disagreement_count"
                    ],
                    **{
                        name: sample["correctness"][name]
                        for name in CORRECTNESS_COUNT_FIELDS
                    },
                }
                nonzero = {name: value for name, value in zero_counts.items() if value != 0}
                if nonzero:
                    raise ValueError(
                        f"{path}: zero-count evidence gate failed for "
                        f"{configuration_key}/{phase_name}/sample "
                        f"{sample['order_index']}: {nonzero}"
                    )


def local_summary(artifacts: list[tuple[Path, dict[str, Any]]]) -> list[dict[str, Any]]:
    keys: set[tuple[int, int, str]] = set()
    by_artifact: list[dict[tuple[int, int, str], dict[str, Any]]] = []
    for _, artifact in artifacts:
        artifact_rows: dict[tuple[int, int, str], dict[str, Any]] = {}
        for configuration in artifact["configurations"]:
            density = configuration["requested_density_percent"]
            macro = configuration["macro_dimension"]
            for phase in ("initial", "after_edit"):
                key = (density, macro, phase)
                keys.add(key)
                artifact_rows[key] = {"configuration": configuration, "phase": configuration[phase]}
        by_artifact.append(artifact_rows)

    output = []
    for density, macro, phase_name in sorted(keys):
        software_times: list[float] = []
        hardware_times: list[float] = []
        candidate_counts: list[float] = []
        rejected_counts: list[float] = []
        committed_counts: list[float] = []
        disagreement_counts: list[float] = []
        traversal_exhausted: list[int] = []
        correctness_counts: dict[str, list[int]] = {
            "false_positive_count": [],
            "false_negative_count": [],
            "wrong_voxel_count": [],
            "hit_t_mismatch_count": [],
        }
        max_t_errors: list[float] = []
        occupied_macro_counts: list[int] = []
        logical_live_resource_bytes: list[int] = []
        peak_heap_usage_bytes: list[int] = []
        build_values: dict[str, list[float]] = {
            "blas_host_ms": [],
            "blas_gpu_ms": [],
            "tlas_host_ms": [],
            "tlas_gpu_ms": [],
            "blas_as_bytes": [],
            "blas_scratch_bytes": [],
        }
        for rows in by_artifact:
            row = rows[(density, macro, phase_name)]
            configuration = row["configuration"]
            phase = row["phase"]
            occupied_macro_counts.append(phase["occupied_macro_count"])
            logical_live_resource_bytes.append(configuration["logical_live_resource_bytes"])
            peak_heap_usage_bytes.append(configuration["peak_device_local_heap_usage_bytes"])
            build_values["blas_host_ms"].append(phase["blas"]["host_build_ms"])
            build_values["blas_gpu_ms"].append(phase["blas"]["gpu_build_ms"])
            build_values["tlas_host_ms"].append(phase["tlas"]["host_build_ms"])
            build_values["tlas_gpu_ms"].append(phase["tlas"]["gpu_build_ms"])
            build_values["blas_as_bytes"].append(phase["blas"]["acceleration_structure_bytes"])
            build_values["blas_scratch_bytes"].append(phase["blas"]["scratch_bytes"])
            for sample in phase["samples"]:
                correctness = sample["correctness"]
                if sample["mode"] == "software":
                    software_times.append(sample["gpu_ms"])
                elif sample["mode"] == "hardware":
                    hardware_times.append(sample["gpu_ms"])
                    candidate_counts.append(sample["candidate_count"])
                    rejected_counts.append(sample["rejected_candidate_count"])
                    committed_counts.append(sample["committed_candidate_count"])
                    disagreement_counts.append(sample["query_committed_disagreement_count"])
                else:
                    raise ValueError(f"unknown sample mode {sample['mode']}")
                traversal_exhausted.append(sample["traversal_exhausted_count"])
                for count_name in correctness_counts:
                    correctness_counts[count_name].append(correctness[count_name])
                max_t_errors.append(correctness["max_hit_t_error"])

        software_stats = stats(software_times)
        hardware_stats = stats(hardware_times)
        candidate_mean = statistics.fmean(candidate_counts)
        rejected_mean = statistics.fmean(rejected_counts)
        output.append(
            {
                "density_percent": density,
                "macro_dimension": macro,
                "phase": phase_name,
                "occupied_macro_count": stats([float(value) for value in occupied_macro_counts]),
                "software_gpu_ms": software_stats,
                "hardware_gpu_ms": hardware_stats,
                "local_speedup_software_over_hardware": software_stats["median"] / hardware_stats["median"],
                "candidate_count": stats(candidate_counts),
                "rejected_candidate_count": stats(rejected_counts),
                "committed_candidate_count": stats(committed_counts),
                "broad_phase_rejection_fraction": rejected_mean / candidate_mean if candidate_mean else 0.0,
                "query_committed_disagreement_count": stats(disagreement_counts),
                "traversal_exhausted_count_max": max(traversal_exhausted),
                "correctness_count_max": {name: max(values) for name, values in correctness_counts.items()},
                "max_hit_t_error": max(max_t_errors),
                "hit_t_tolerance": by_artifact[0][(density, macro, phase_name)]["phase"]["samples"][0]["correctness"]["hit_t_tolerance"],
                "build": {name: stats(values) for name, values in build_values.items()},
                "logical_live_resource_bytes": stats([float(value) for value in logical_live_resource_bytes]),
                "peak_device_local_heap_usage_bytes": stats([float(value) for value in peak_heap_usage_bytes]),
            }
        )
    return output


def parse_label_path(value: str) -> tuple[str, Path]:
    label, separator, path = value.partition("=")
    if not separator or not label or not path:
        raise argparse.ArgumentTypeError("expected LABEL=PATH")
    return label, Path(path)


def command_output(command: list[str]) -> str:
    try:
        return subprocess.run(command, check=True, capture_output=True, text=True).stdout.strip()
    except (FileNotFoundError, subprocess.CalledProcessError) as error:
        return f"unavailable: {error}"


def vulkan_ray_tracing_limits() -> dict[str, int | str]:
    output = command_output(["vulkaninfo"])
    if output.startswith("unavailable:"):
        return {"error": output}
    names = (
        "maxGeometryCount",
        "maxInstanceCount",
        "maxPrimitiveCount",
        "minAccelerationStructureScratchOffsetAlignment",
        "maxRayRecursionDepth",
        "maxRayDispatchInvocationCount",
    )
    values: dict[str, int | str] = {}
    for name in names:
        match = re.search(rf"^\s*{name}\s*=\s*(\d+)\s*$", output, re.MULTILINE)
        values[name] = int(match.group(1)) if match else "unavailable"
    return values


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--ray-query-artifact", action="append", type=Path, required=True)
    parser.add_argument("--frame-run", action="append", type=parse_label_path, required=True)
    parser.add_argument("--binary-a", type=Path, required=True)
    parser.add_argument("--binary-b", type=Path, required=True)
    parser.add_argument("--tail-samples", type=int, default=64)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    if len(args.ray_query_artifact) != 2 or len(
        {path.resolve() for path in args.ray_query_artifact}
    ) != 2:
        raise ValueError("expected exactly two independent ray-query artifacts")

    artifact_rows = []
    artifact_metadata = []
    for path in args.ray_query_artifact:
        with path.open("rb") as source:
            artifact = tomllib.load(source)
        validate_ray_query_artifact(path, artifact)
        artifact_rows.append((path, artifact))
        artifact_metadata.append(
            {
                "path": str(path),
                "sha256": sha256(path),
                "schema": artifact["schema"],
                "generated_at": artifact["generated_at"],
                "command": artifact["command"],
            }
        )
    if (
        len({metadata["sha256"] for metadata in artifact_metadata}) != 2
        or len({metadata["generated_at"] for metadata in artifact_metadata}) != 2
        or len({tuple(metadata["command"]) for metadata in artifact_metadata}) != 2
    ):
        raise ValueError("expected exactly two independent ray-query artifacts")
    first_artifact = artifact_rows[0][1]
    for _, artifact in artifact_rows[1:]:
        if artifact["machine"] != first_artifact["machine"] or artifact["workload"] != first_artifact["workload"]:
            raise ValueError("ray-query artifacts do not share machine/workload identity")
    for capability in (
        "acceleration_structure_extension",
        "ray_query_extension",
        "acceleration_structure_feature",
        "ray_query_feature",
    ):
        if not first_artifact["machine"][capability]:
            raise ValueError(f"required hardware capability is false: {capability}")

    frame_labels = tuple(label for label, _ in args.frame_run)
    frame_paths = tuple(path.resolve() for _, path in args.frame_run)
    if frame_labels != EXPECTED_FRAME_RUN_ORDER or len(set(frame_paths)) != len(
        EXPECTED_FRAME_RUN_ORDER
    ):
        raise ValueError(
            "frame evidence must be exactly four distinct logs in A1/B1/B2/A2 order"
        )
    frame_runs = {label: parse_frame_log(path, args.tail_samples) for label, path in args.frame_run}

    arm_frames: dict[str, dict[str, Any]] = {}
    for arm, labels in {"A_default": ("A1", "A2"), "B_rtx_feature_post_benchmark": ("B1", "B2")}.items():
        arm_frames[arm] = {}
        for scope in FRAME_SCOPES:
            combined = [
                sample[scope]
                for label in labels
                for sample in frame_runs[label]["selected_samples"]
            ]
            arm_frames[arm][scope] = stats(combined)
    frame_speedups = {
        scope: arm_frames["A_default"][scope]["median"]
        / arm_frames["B_rtx_feature_post_benchmark"][scope]["median"]
        for scope in FRAME_SCOPES
    }

    local = local_summary(artifact_rows)
    for row in local:
        hardware_evidence = {
            "candidate_count": row["candidate_count"],
            "committed_candidate_count": row["committed_candidate_count"],
            "blas_gpu_ms": row["build"]["blas_gpu_ms"],
            "tlas_gpu_ms": row["build"]["tlas_gpu_ms"],
        }
        non_positive = [name for name, values in hardware_evidence.items() if values["min"] <= 0]
        if non_positive:
            raise ValueError(f"hardware evidence minimum is not positive for {non_positive}: {row}")
        committed_disagreement = row["query_committed_disagreement_count"]
        if committed_disagreement["min"] != 0 or committed_disagreement["max"] != 0:
            raise ValueError(f"committed query disagreement gate failed: {row}")
        if row["traversal_exhausted_count_max"] != 0 or any(row["correctness_count_max"].values()):
            raise ValueError(f"correctness gate failed: {row}")
    best_local = max(row["local_speedup_software_over_hardware"] for row in local)
    baseline_frame = arm_frames["A_default"]["frame.render"]["median"]
    tracer_pass_fraction = arm_frames["A_default"]["tracer.pass"]["median"] / baseline_frame
    tracer_render_fraction = arm_frames["A_default"]["tracer.render"]["median"] / baseline_frame

    def amdahl(fraction: float, local_speedup: float) -> float:
        return 1.0 / ((1.0 - fraction) + fraction / local_speedup)

    result = {
        "schema": "re-flora.rtx-voxel-hardware-ray-query-summary.v1",
        "fixed_baseline": first_artifact["fixed_baseline"],
        "method": {
            "local_sample_order": first_artifact["workload"]["sample_order"],
            "frame_run_order": ["A1", "B1", "B2", "A2"],
            "frame_tail_sample_count_per_run": args.tail_samples,
            "frame_sample_cadence": "one GPU_FRAME_SCOPE record per 30 rendered frames",
            "frame_units": "milliseconds",
            "local_units": "milliseconds",
            "frame_warning": "B initializes, runs, and drops the one-shot ray-query experiment before normal frames; this comparison is not an integrated RTX renderer speedup.",
            "commands": {
                "build_A": ["cargo", "build", "--release"],
                "build_B": ["cargo", "build", "--release", "--features", "rtx-voxel-experiment"],
                "frame_A": [
                    "target/release/re-flora-rtx-a",
                    "--hidden",
                    "--mute",
                    "--perf",
                    "--camera-snapshot",
                    "player-default",
                    "--auto-exit",
                    "8",
                ],
                "frame_B_extra_arguments": [
                    "--rtx-voxel-benchmark",
                    "docs/evidence/rtx_voxel_hardware_rt/raw/rtx_b[12].toml",
                ],
            },
        },
        "machine": {
            **first_artifact["machine"],
            "nvidia_smi": command_output(
                [
                    "nvidia-smi",
                    "--query-gpu=name,driver_version,memory.total,pci.bus_id",
                    "--format=csv,noheader",
                ]
            ),
            "vulkan_ray_tracing_limits": vulkan_ray_tracing_limits(),
            "kernel": platform.uname()._asdict(),
        },
        "workload": first_artifact["workload"],
        "binaries": {
            "A_default": {"path": str(args.binary_a), "sha256": sha256(args.binary_a)},
            "B_rtx_feature": {"path": str(args.binary_b), "sha256": sha256(args.binary_b)},
        },
        "ray_query_artifacts": artifact_metadata,
        "local_results": local,
        "frame_runs": frame_runs,
        "frame_arms": arm_frames,
        "frame_speedup_A_over_B": frame_speedups,
        "amdahl": {
            "best_observed_correct_local_speedup": best_local,
            "baseline_frame_render_median_ms": baseline_frame,
            "tracer_pass_fraction": tracer_pass_fraction,
            "tracer_render_fraction": tracer_render_fraction,
            "bound_if_only_tracer_pass_gets_best_local_speedup": amdahl(tracer_pass_fraction, best_local),
            "bound_if_all_tracer_render_gets_best_local_speedup": amdahl(tracer_render_fraction, best_local),
            "infinite_speedup_bound_tracer_pass": 1.0 / (1.0 - tracer_pass_fraction),
            "infinite_speedup_bound_tracer_render": 1.0 / (1.0 - tracer_render_fraction),
            "warning": "Illustrative bound only: synthetic DDA and production Contree traversal are not equivalent workloads.",
        },
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
