#!/usr/bin/env python3
"""Fail-closed recomputation for the disposable static RTX tracer bullet."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import statistics
import tomllib
from pathlib import Path
from typing import Any


SCHEMA = "re-flora.rtx-static-tracer-bullet.v1"
SUMMARY_SCHEMA = "re-flora.rtx-static-tracer-bullet-summary.v1"
FIXED_BASELINE = "7ce60e06f1b70793c18339ce60a59a61c985aa82"
EXPECTED_VOLUMES = (
    "sparse_5_percent_with_fixture",
    "dense_75_percent_with_fixture",
    "shell_cavity",
)
EXPECTED_MODES = ("software_dda", "voxel_aabb_exact", "exposed_face_triangles")
SAMPLE_BLOCK = (
    "software_dda",
    "voxel_aabb_exact",
    "exposed_face_triangles",
    "exposed_face_triangles",
    "voxel_aabb_exact",
    "software_dda",
    "exposed_face_triangles",
    "voxel_aabb_exact",
    "software_dda",
    "software_dda",
    "voxel_aabb_exact",
    "exposed_face_triangles",
)
EXPECTED_SAMPLE_ORDER = SAMPLE_BLOCK * 3
CORRECTNESS_FIELDS = (
    "false_positive_count",
    "committed_false_positive_count",
    "false_negative_count",
    "wrong_voxel_count",
    "wrong_face_count",
    "wrong_normal_count",
    "primitive_mapping_mismatch_count",
    "hit_t_mismatch_count",
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def percentile(values: list[float], quantile: float) -> float:
    ordered = sorted(values)
    if not ordered:
        raise ValueError("cannot summarize an empty sample")
    if len(ordered) == 1:
        return ordered[0]
    position = (len(ordered) - 1) * quantile
    lower = math.floor(position)
    upper = math.ceil(position)
    if lower == upper:
        return ordered[lower]
    weight = position - lower
    return ordered[lower] * (1.0 - weight) + ordered[upper] * weight


def stats(values: list[float | int]) -> dict[str, float | int]:
    if not values:
        raise ValueError("cannot summarize an empty sample")
    numeric = [float(value) for value in values]
    return {
        "count": len(numeric),
        "mean": statistics.fmean(numeric),
        "median": statistics.median(numeric),
        "p95": percentile(numeric, 0.95),
        "min": min(numeric),
        "max": max(numeric),
    }


def require_positive(value: float | int, context: str) -> None:
    if value <= 0:
        raise ValueError(f"{context}: expected positive evidence, got {value}")


def validate_workload(path: Path, artifact: dict[str, Any]) -> None:
    workload = artifact.get("workload", {})
    expected_scalars = {
        "world_dimension": 64,
        "ray_grid_width": 1024,
        "ray_grid_height": 1024,
        "ray_count": 1_048_576,
        "candidate_budget": 4096,
        "max_dda_steps": 256,
        "warmups_per_mode": 2,
    }
    actual_scalars = {name: workload.get(name) for name in expected_scalars}
    if actual_scalars != expected_scalars:
        raise ValueError(f"{path}: workload scalars differ: {actual_scalars}")
    if tuple(workload.get("sample_order", ())) != EXPECTED_SAMPLE_ORDER:
        raise ValueError(f"{path}: workload sample sequence differs")
    if tuple(workload.get("volumes", ())) != EXPECTED_VOLUMES:
        raise ValueError(f"{path}: workload volume Cartesian product differs")


def validate_build(path: Path, volume: dict[str, Any]) -> None:
    name = volume["name"]
    build = volume["build"]
    surface = volume["surface_voxel_count"]
    faces = volume["exposed_face_count"]
    triangles = volume["triangle_primitive_count"]
    triangle_blas = build["triangle_blas"]
    triangle_tlas = build["triangle_tlas"]
    aabb_blas = build["aabb_blas"]
    aabb_tlas = build["aabb_tlas"]

    triangle_shape = (
        triangles == faces * 2
        and triangle_blas["primitive_count"] == triangles
        and triangle_tlas["primitive_count"] == 1
        and build["triangle_vertex_input_bytes"] == faces * 4 * 12
        and build["triangle_index_input_bytes"] == faces * 6 * 4
        and build["triangle_metadata_bytes"] == faces * 16
    )
    if not triangle_shape:
        raise ValueError(f"{path}: triangle evidence shape/count failed for {name}")
    for field in ("triangle_extraction_host_ms", "triangle_vertex_input_bytes", "triangle_index_input_bytes"):
        require_positive(build[field], f"{path}: triangle evidence {name}/{field}")
    for label, row in (("BLAS", triangle_blas), ("TLAS", triangle_tlas)):
        for field in ("host_build_ms", "gpu_build_ms", "acceleration_structure_bytes", "scratch_bytes"):
            require_positive(row[field], f"{path}: triangle evidence {name}/{label}/{field}")

    aabb_shape = (
        aabb_blas["primitive_count"] == surface
        and aabb_tlas["primitive_count"] == 1
        and build["aabb_input_bytes"] == surface * 24
        and build["aabb_metadata_bytes"] == surface * 4
    )
    if not aabb_shape:
        raise ValueError(f"{path}: AABB candidate evidence shape/count failed for {name}")
    for field in ("aabb_extraction_host_ms", "aabb_input_bytes", "aabb_metadata_bytes"):
        require_positive(build[field], f"{path}: AABB candidate evidence {name}/{field}")
    for label, row in (("BLAS", aabb_blas), ("TLAS", aabb_tlas)):
        for field in ("host_build_ms", "gpu_build_ms", "acceleration_structure_bytes", "scratch_bytes"):
            require_positive(row[field], f"{path}: AABB candidate evidence {name}/{label}/{field}")
    for field in ("static_live_resource_bytes", "build_peak_accounted_bytes", "peak_device_local_heap_usage_bytes"):
        require_positive(volume[field], f"{path}: memory evidence {name}/{field}")


def validate_sample(path: Path, volume_name: str, sample: dict[str, Any]) -> None:
    mode = sample["mode"]
    context = f"{path}: {volume_name}/{mode}/sample-{sample['order_index']}"
    if sample["ray_count"] != 1_048_576 or sample["correctness"]["reference_ray_count"] != 1_048_576:
        raise ValueError(f"{context}: ray/reference count differs")
    require_positive(sample["gpu_ms"], f"{context}: {('triangle evidence' if mode == 'exposed_face_triangles' else 'traversal evidence')} GPU time")
    require_positive(sample["host_wait_ms"], f"{context}: host wait time")
    require_positive(sample["hit_count"], f"{context}: hit count")

    zero_counts = {
        "traversal_exhausted_count": sample["traversal_exhausted_count"],
        "committed_disagreement_count": sample["committed_disagreement_count"],
        **{field: sample["correctness"][field] for field in CORRECTNESS_FIELDS},
    }
    nonzero = {name: value for name, value in zero_counts.items() if value != 0}
    if nonzero:
        raise ValueError(f"{context}: zero-count correctness gate failed: {nonzero}")
    if sample["correctness"].get("first_mismatches"):
        raise ValueError(f"{context}: zero-count correctness gate failed: mismatch details remain")
    if sample["correctness"]["max_hit_t_error"] > sample["correctness"]["hit_t_tolerance"]:
        raise ValueError(f"{context}: zero-count correctness gate failed: t error exceeds tolerance")

    counters = {name: sample[name] for name in (
        "candidate_count",
        "rejected_candidate_count",
        "confirmed_candidate_count",
        "generated_candidate_count",
        "committed_candidate_count",
    )}
    if mode == "software_dda":
        if any(counters.values()):
            raise ValueError(f"{context}: software candidate counters are nonzero: {counters}")
    elif mode == "exposed_face_triangles":
        if any(counters[name] for name in ("candidate_count", "rejected_candidate_count", "confirmed_candidate_count", "generated_candidate_count")):
            raise ValueError(f"{context}: triangle evidence contains procedural candidates: {counters}")
        if counters["committed_candidate_count"] != sample["hit_count"]:
            raise ValueError(f"{context}: triangle evidence committed/hit count differs")
        require_positive(counters["committed_candidate_count"], f"{context}: triangle evidence committed count")
    elif mode == "voxel_aabb_exact":
        valid = (
            counters["candidate_count"] > 0
            and counters["rejected_candidate_count"] > 0
            and counters["confirmed_candidate_count"] > 0
            and counters["generated_candidate_count"] > 0
            and counters["committed_candidate_count"] > 0
            and counters["candidate_count"]
            == counters["rejected_candidate_count"] + counters["confirmed_candidate_count"]
            and counters["generated_candidate_count"] <= counters["confirmed_candidate_count"]
            and counters["committed_candidate_count"] == sample["hit_count"]
        )
        if not valid:
            raise ValueError(f"{context}: AABB candidate evidence confirmation failed: {counters}")
    else:
        raise ValueError(f"{context}: unknown traversal mode")


def validate_artifact(path: Path, artifact: dict[str, Any]) -> None:
    if artifact.get("schema") != SCHEMA or artifact.get("fixed_baseline") != FIXED_BASELINE:
        raise ValueError(f"{path}: schema/baseline differs")
    if "PROTOTYPE/TRACER BULLET" not in artifact.get("prototype_notice", ""):
        raise ValueError(f"{path}: prototype notice missing")
    validate_workload(path, artifact)
    capability_names = (
        "acceleration_structure_extension",
        "ray_query_extension",
        "ray_tracing_pipeline_extension",
        "acceleration_structure_feature",
        "ray_query_feature",
    )
    for capability in capability_names:
        if artifact.get("machine", {}).get(capability) is not True:
            raise ValueError(f"{path}: hardware capability is not true: {capability}")

    volumes = artifact.get("volumes", [])
    actual_names = [volume.get("name") for volume in volumes]
    if len(actual_names) != len(EXPECTED_VOLUMES) or set(actual_names) != set(EXPECTED_VOLUMES):
        raise ValueError(f"{path}: volume Cartesian product is missing, duplicated, or unexpected: {actual_names}")
    expected_sequence = list(enumerate(EXPECTED_SAMPLE_ORDER))
    for volume in volumes:
        validate_build(path, volume)
        actual_sequence = [(sample.get("order_index"), sample.get("mode")) for sample in volume.get("samples", [])]
        if actual_sequence != expected_sequence:
            raise ValueError(f"{path}: sample sequence differs for {volume['name']}: {actual_sequence}")
        hit_counts: dict[str, set[int]] = {mode: set() for mode in EXPECTED_MODES}
        for sample in volume["samples"]:
            validate_sample(path, volume["name"], sample)
            hit_counts[sample["mode"]].add(sample["hit_count"])
        if any(len(values) != 1 for values in hit_counts.values()) or len({next(iter(values)) for values in hit_counts.values()}) != 1:
            raise ValueError(f"{path}: mode hit-count agreement failed for {volume['name']}: {hit_counts}")


def validate_run_log(path: Path, artifact: dict[str, Any]) -> dict[str, str]:
    text = path.read_text(encoding="utf-8")
    required = (
        "[VKN][HARDWARE_RAY_QUERY] enabled",
        "[RTX_STATIC_TRACER_BULLET][PROTOTYPE][CAPABILITY]",
        "as_ext=true",
        "ray_query_ext=true",
        "as_feature=true",
        "ray_query_feature=true",
        "[RTX_STATIC_TRACER_BULLET][PROTOTYPE][COMPLETE]",
        "resources=dropped-before-production",
        "[SHUTDOWN] phase=complete failures=0",
        "Application exited successfully",
    )
    missing = [marker for marker in required if marker not in text]
    for volume in EXPECTED_VOLUMES:
        if f"[PROTOTYPE][RESULT] volume={volume}" not in text:
            missing.append(f"result:{volume}")
    output_name = Path(artifact["command"][artifact["command"].index("--rtx-static-tracer-bullet") + 1]).name
    if output_name not in text:
        missing.append(f"artifact:{output_name}")
    if missing:
        raise ValueError(f"{path}: runtime-authority evidence is missing: {missing}")
    return {"path": str(path), "sha256": sha256(path)}


def build_summary(
    artifacts: list[tuple[Path, dict[str, Any]]],
    artifact_metadata: list[dict[str, Any]],
    log_metadata: list[dict[str, str]],
    binary_sha256: str,
    source_head: str,
) -> dict[str, Any]:
    rows = []
    for volume_name in EXPECTED_VOLUMES:
        volume_rows = [
            next(volume for volume in artifact["volumes"] if volume["name"] == volume_name)
            for _, artifact in artifacts
        ]
        count_fields = ("occupied_voxel_count", "surface_voxel_count", "exposed_face_count", "triangle_primitive_count")
        static_counts = {field: {volume[field] for volume in volume_rows} for field in count_fields}
        if any(len(values) != 1 for values in static_counts.values()):
            raise ValueError(f"{volume_name}: static geometry counts differ across artifacts: {static_counts}")

        mode_results: dict[str, Any] = {}
        per_artifact_medians: dict[str, list[float]] = {mode: [] for mode in EXPECTED_MODES}
        for mode in EXPECTED_MODES:
            samples = [sample for volume in volume_rows for sample in volume["samples"] if sample["mode"] == mode]
            times = [sample["gpu_ms"] for sample in samples]
            for volume in volume_rows:
                per_artifact_medians[mode].append(statistics.median(
                    sample["gpu_ms"] for sample in volume["samples"] if sample["mode"] == mode
                ))
            mode_results[mode] = {
                "gpu_ms": stats(times),
                "ns_per_ray": stats([sample["ns_per_ray"] for sample in samples]),
                "mrays_per_second": stats([sample["mrays_per_second"] for sample in samples]),
                "hit_count": stats([sample["hit_count"] for sample in samples]),
                "candidate_count": stats([sample["candidate_count"] for sample in samples]),
                "rejected_candidate_count": stats([sample["rejected_candidate_count"] for sample in samples]),
                "confirmed_candidate_count": stats([sample["confirmed_candidate_count"] for sample in samples]),
                "generated_candidate_count": stats([sample["generated_candidate_count"] for sample in samples]),
                "committed_candidate_count": stats([sample["committed_candidate_count"] for sample in samples]),
                "max_hit_t_error": max(sample["correctness"]["max_hit_t_error"] for sample in samples),
                "hit_t_tolerance": samples[0]["correctness"]["hit_t_tolerance"],
                "correctness_count_max": {
                    field: max(sample["correctness"][field] for sample in samples)
                    for field in CORRECTNESS_FIELDS
                },
                "traversal_exhausted_count_max": max(sample["traversal_exhausted_count"] for sample in samples),
                "committed_disagreement_count_max": max(sample["committed_disagreement_count"] for sample in samples),
            }

        software_median = mode_results["software_dda"]["gpu_ms"]["median"]
        triangle_median = mode_results["exposed_face_triangles"]["gpu_ms"]["median"]
        aabb_median = mode_results["voxel_aabb_exact"]["gpu_ms"]["median"]
        triangle_per_artifact = [
            software / triangle
            for software, triangle in zip(
                per_artifact_medians["software_dda"],
                per_artifact_medians["exposed_face_triangles"],
                strict=True,
            )
        ]
        aabb_per_artifact = [
            software / aabb
            for software, aabb in zip(
                per_artifact_medians["software_dda"],
                per_artifact_medians["voxel_aabb_exact"],
                strict=True,
            )
        ]
        aabb_samples = [
            sample
            for volume in volume_rows
            for sample in volume["samples"]
            if sample["mode"] == "voxel_aabb_exact"
        ]
        total_candidates = sum(sample["candidate_count"] for sample in aabb_samples)
        total_rejected = sum(sample["rejected_candidate_count"] for sample in aabb_samples)
        build_fields = (
            "triangle_extraction_host_ms",
            "aabb_extraction_host_ms",
        )
        build_summary = {field: stats([volume["build"][field] for volume in volume_rows]) for field in build_fields}
        for structure in ("triangle_blas", "triangle_tlas", "aabb_blas", "aabb_tlas"):
            build_summary[structure] = {
                field: stats([volume["build"][structure][field] for volume in volume_rows])
                for field in ("host_build_ms", "gpu_build_ms", "acceleration_structure_bytes", "scratch_bytes")
            }
        rows.append(
            {
                "volume": volume_name,
                **{field: next(iter(values)) for field, values in static_counts.items()},
                "actual_density_percent": stats([volume["actual_density_percent"] for volume in volume_rows]),
                "modes": mode_results,
                "triangle_speedup_software_over_triangle": software_median / triangle_median,
                "triangle_speedup_per_artifact": triangle_per_artifact,
                "aabb_speedup_software_over_aabb": software_median / aabb_median,
                "aabb_speedup_per_artifact": aabb_per_artifact,
                "aabb_broad_phase_rejection_fraction": total_rejected / total_candidates,
                "build": build_summary,
                "memory": {
                    field: stats([volume[field] for volume in volume_rows])
                    for field in ("static_live_resource_bytes", "build_peak_accounted_bytes", "peak_device_local_heap_usage_bytes")
                },
            }
        )

    best = max(rows, key=lambda row: row["triangle_speedup_software_over_triangle"])
    best_speedup = best["triangle_speedup_software_over_triangle"]

    def amdahl(fraction: float) -> float:
        return 1.0 / ((1.0 - fraction) + fraction / best_speedup)

    first_artifact = artifacts[0][1]
    return {
        "schema": SUMMARY_SCHEMA,
        "verdict": "NO-GO_ORDER_OF_MAGNITUDE_STATIC_TRAVERSAL",
        "highest_credible_triangle_speedup": best_speedup,
        "highest_credible_triangle_speedup_volume": best["volume"],
        "fixed_baseline": FIXED_BASELINE,
        "source_head": source_head,
        "binary_sha256": binary_sha256,
        "prototype_notice": first_artifact["prototype_notice"],
        "method": {
            "artifact_count": 2,
            "samples_per_mode_per_artifact_per_volume": 12,
            "pooled_samples_per_mode_per_volume": 24,
            "warmups_per_mode": 2,
            "sample_order": list(EXPECTED_SAMPLE_ORDER),
            "units": "GPU timestamp milliseconds; ns/ray; Mray/s",
            "scope": "static build-once synthetic traversal only; no production whole-frame claim",
        },
        "machine": first_artifact["machine"],
        "workload": first_artifact["workload"],
        "artifacts": artifact_metadata,
        "run_logs": log_metadata,
        "results": rows,
        "illustrative_amdahl_only": {
            "local_speedup": best_speedup,
            "assumed_replaced_fraction_to_bound": {
                str(fraction): amdahl(fraction) for fraction in (0.25, 0.5, 0.75, 1.0)
            },
            "warning": "No production fraction was measured; these are hypothetical bounds, not frame predictions.",
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifact", action="append", type=Path, required=True)
    parser.add_argument("--run-log", action="append", type=Path, required=True)
    parser.add_argument("--binary-sha256", required=True)
    parser.add_argument("--source-head", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    if (
        len(args.artifact) != 2
        or len({path.resolve() for path in args.artifact}) != 2
        or len(args.run_log) != 2
        or len({path.resolve() for path in args.run_log}) != 2
    ):
        raise ValueError("expected exactly two independent artifacts and run logs")
    if len(args.binary_sha256) != 64 or any(character not in "0123456789abcdef" for character in args.binary_sha256):
        raise ValueError("binary SHA-256 must be 64 lowercase hexadecimal characters")
    if len(args.source_head) != 40 or any(character not in "0123456789abcdef" for character in args.source_head):
        raise ValueError("source HEAD must be 40 lowercase hexadecimal characters")

    artifacts: list[tuple[Path, dict[str, Any]]] = []
    metadata: list[dict[str, Any]] = []
    for path in args.artifact:
        with path.open("rb") as source:
            artifact = tomllib.load(source)
        validate_artifact(path, artifact)
        artifacts.append((path, artifact))
        metadata.append(
            {
                "path": str(path),
                "sha256": sha256(path),
                "schema": artifact["schema"],
                "generated_at": artifact["generated_at"],
                "command": artifact["command"],
            }
        )
    if (
        len({row["sha256"] for row in metadata}) != 2
        or len({row["generated_at"] for row in metadata}) != 2
        or len({tuple(row["command"]) for row in metadata}) != 2
    ):
        raise ValueError("expected exactly two independent artifacts and run logs")
    first = artifacts[0][1]
    for _, artifact in artifacts[1:]:
        if artifact["machine"] != first["machine"] or artifact["workload"] != first["workload"]:
            raise ValueError("artifacts do not share machine/workload identity")

    log_metadata = [
        validate_run_log(log_path, artifact)
        for log_path, (_, artifact) in zip(args.run_log, artifacts, strict=True)
    ]
    if len({row["sha256"] for row in log_metadata}) != 2:
        raise ValueError("expected exactly two independent artifacts and run logs")

    result = build_summary(
        artifacts,
        metadata,
        log_metadata,
        args.binary_sha256,
        args.source_head,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
