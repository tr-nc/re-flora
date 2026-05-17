#!/usr/bin/env python3
"""Summarize Re: Flora perf logs into compact markdown tables."""

from __future__ import annotations

import argparse
import re
import statistics
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
LOG_DIR = ROOT / "target" / "re-flora-logs"


FRAME_DETAIL_RE = re.compile(r"\[PERF\]\[FRAME\] frame (\d+) ")
FRAME_RE = re.compile(
    r"\[PERF\] frame (\d+) total ([0-9.]+)ms egui ([0-9.]+)ms gpu\+present ([0-9.]+)ms"
)
WATER_RE = re.compile(
    r"\[PERF\]\[WATER\] particles (\d+) .*? substeps (\d+) total ([0-9.]+)ms avg ([0-9.]+)ms/substep"
)
DEFERRED_REBUILD_RE = re.compile(r"\[PERF\]\[DEFERRED_REBUILD\].* total ([0-9.]+)ms")
SYNC_VISIBLE_REBUILD_RE = re.compile(r"\[PERF\]\[SYNC_VISIBLE_REBUILD\]")
DEFERRED_REBUILD_PHASE_RE = re.compile(r"\[PERF\]\[DEFERRED_REBUILD_PHASE\]")
SURFACE_BUILD_RE = re.compile(r"\[PERF\]\[SURFACE_BUILD\]")
CONTREE_REBUILD_RE = re.compile(r"\[QUEUE\]\[CONTREE_REBUILD\]")
SOURCE_REFRESH_RE = re.compile(r"refreshed GPU solid source")
COLLIDER_BUILD_RE = re.compile(r"built collider chunk .* build_ms=([0-9.]+)")
CACHE_APPLY_RE = re.compile(r"applied worker grid cache region .* worker_ms=([0-9.]+) apply_ms=([0-9.]+)")
CACHE_DISCARD_RE = re.compile(r"discarded stale worker grid cache region .* worker_ms=([0-9.]+)")

FRAME_MS_FIELDS = [
    "total",
    "egui",
    "gpu_present",
    "contree_poll",
    "terrain_source",
    "deferred_rebuild",
    "cache_queue",
    "collider_queue",
    "water_edit_soak",
    "water_update",
    "particles",
    "tracked_cpu",
    "untracked_cpu",
]

WATER_MS_FIELDS = [
    "total",
    "repair",
    "clear",
    "p2g",
    "grid",
    "grid_update",
    "pressure",
    "g2p",
    "g2p_gather",
    "g2p_box",
    "g2p_terrain",
    "g2p_repair",
    "spacing_relax",
    "diagnostics",
    "residual",
    "shadow_measure",
]


def latest_log() -> Path:
    logs = sorted(LOG_DIR.glob("*.log"), key=lambda path: path.stat().st_mtime)
    if not logs:
        raise SystemExit(f"no logs found under {LOG_DIR}")
    return logs[-1]


def extract_ms(line: str, field: str) -> float | None:
    match = re.search(rf"\b{re.escape(field)} ([0-9.]+)ms", line)
    if not match:
        return None
    return float(match.group(1))


def extract_eq_ms(line: str, field: str) -> float | None:
    match = re.search(rf"\b{re.escape(field)}=([0-9.]+)ms", line)
    if not match:
        return None
    return float(match.group(1))


def percentile(values: list[float], pct: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    idx = int(round((len(ordered) - 1) * pct))
    return ordered[idx]


def summarize(values: list[float]) -> dict[str, float]:
    if not values:
        return {"n": 0, "sum": 0.0, "mean": 0.0, "median": 0.0, "p95": 0.0, "max": 0.0}
    return {
        "n": len(values),
        "sum": sum(values),
        "mean": statistics.fmean(values),
        "median": statistics.median(values),
        "p95": percentile(values, 0.95),
        "max": max(values),
    }


def print_stats_table(title: str, rows: list[tuple[str, list[float]]]) -> None:
    print(f"\n## {title}\n")
    print("| metric | n | sum ms | mean ms | median ms | p95 ms | max ms |")
    print("|---|---:|---:|---:|---:|---:|---:|")
    for name, values in rows:
        stats = summarize(values)
        if stats["n"] == 0:
            continue
        print(
            f"| {name} | {stats['n']:.0f} | {stats['sum']:.2f} | {stats['mean']:.2f} | "
            f"{stats['median']:.2f} | {stats['p95']:.2f} | {stats['max']:.2f} |"
        )


def parse_log(path: Path) -> None:
    frame_detail: dict[str, list[float]] = {field: [] for field in FRAME_MS_FIELDS}
    frame_basic: dict[str, list[float]] = {"total": [], "egui": [], "gpu_present": [], "cpu_other": []}
    water: dict[str, list[float]] = {field: [] for field in WATER_MS_FIELDS}
    water_particles: list[float] = []
    water_avg_substep: list[float] = []
    deferred_rebuild: list[float] = []
    sync_visible_rebuild: list[float] = []
    deferred_surface_total: list[float] = []
    deferred_contree_total: list[float] = []
    deferred_scene_total: list[float] = []
    deferred_scene_finish: list[float] = []
    deferred_preserve_flora: list[float] = []
    surface_build_total: list[float] = []
    surface_build_fence_latency: list[float] = []
    surface_build_flora: list[float] = []
    contree_rebuild_total: list[float] = []
    contree_rebuild_fence_latency: list[float] = []
    contree_rebuild_size: list[float] = []
    contree_rebuild_confirm: list[float] = []
    source_refresh: list[float] = []
    source_gpu_sample_total: list[float] = []
    source_fence_latency: list[float] = []
    source_gpu_submit: list[float] = []
    source_gpu_readback: list[float] = []
    collider_build: list[float] = []
    cache_worker: list[float] = []
    cache_apply: list[float] = []
    cache_discard_worker: list[float] = []

    for line in path.read_text(errors="replace").splitlines():
        if FRAME_DETAIL_RE.search(line):
            for field in FRAME_MS_FIELDS:
                value = extract_ms(line, field)
                if value is not None:
                    frame_detail[field].append(value)
            continue

        if match := FRAME_RE.search(line):
            total = float(match.group(2))
            egui = float(match.group(3))
            gpu = float(match.group(4))
            frame_basic["total"].append(total)
            frame_basic["egui"].append(egui)
            frame_basic["gpu_present"].append(gpu)
            frame_basic["cpu_other"].append(max(0.0, total - egui - gpu))
            continue

        if match := WATER_RE.search(line):
            water_particles.append(float(match.group(1)))
            water["total"].append(float(match.group(3)))
            water_avg_substep.append(float(match.group(4)))
            for field in WATER_MS_FIELDS:
                if field == "total":
                    continue
                value = extract_ms(line, field)
                if value is not None:
                    water[field].append(value)
            continue

        if SYNC_VISIBLE_REBUILD_RE.search(line):
            if (value := extract_ms(line, "total")) is not None:
                sync_visible_rebuild.append(value)
            continue
        if match := DEFERRED_REBUILD_RE.search(line):
            deferred_rebuild.append(float(match.group(1)))
            if (value := extract_ms(line, "surface_total")) is not None:
                deferred_surface_total.append(value)
            if (value := extract_ms(line, "contree_total")) is not None:
                deferred_contree_total.append(value)
            if (value := extract_ms(line, "scene_total")) is not None:
                deferred_scene_total.append(value)
            if (value := extract_ms(line, "scene_finish")) is not None:
                deferred_scene_finish.append(value)
            continue
        if DEFERRED_REBUILD_PHASE_RE.search(line):
            if (value := extract_ms(line, "preserve_flora")) is not None and value > 0.0:
                deferred_preserve_flora.append(value)
            continue
        if SURFACE_BUILD_RE.search(line):
            if (value := extract_ms(line, "total")) is not None:
                surface_build_total.append(value)
            if (value := extract_ms(line, "fence_latency")) is not None:
                surface_build_fence_latency.append(value)
            if (value := extract_ms(line, "flora")) is not None:
                surface_build_flora.append(value)
            continue
        if CONTREE_REBUILD_RE.search(line):
            if (value := extract_eq_ms(line, "total_ms")) is not None:
                contree_rebuild_total.append(value)
            if (value := extract_eq_ms(line, "fence_latency_ms")) is not None:
                contree_rebuild_fence_latency.append(value)
            if (value := extract_eq_ms(line, "size_ms")) is not None:
                contree_rebuild_size.append(value)
            if (value := extract_eq_ms(line, "confirm_ms")) is not None:
                contree_rebuild_confirm.append(value)
            continue
        if SOURCE_REFRESH_RE.search(line):
            if (value := extract_eq_ms(line, "total")) is not None:
                source_refresh.append(value)
            if (value := extract_eq_ms(line, "gpu_sample_total")) is not None:
                source_gpu_sample_total.append(value)
            if (value := extract_eq_ms(line, "fence_latency")) is not None:
                source_fence_latency.append(value)
            if (value := extract_eq_ms(line, "gpu_submit")) is not None:
                source_gpu_submit.append(value)
            if (value := extract_eq_ms(line, "gpu_readback")) is not None:
                source_gpu_readback.append(value)
            continue
        if match := COLLIDER_BUILD_RE.search(line):
            collider_build.append(float(match.group(1)))
            continue
        if match := CACHE_APPLY_RE.search(line):
            cache_worker.append(float(match.group(1)))
            cache_apply.append(float(match.group(2)))
            continue
        if match := CACHE_DISCARD_RE.search(line):
            cache_discard_worker.append(float(match.group(1)))
            continue

    print(f"# Perf log summary\n\n`{path}`")
    if any(frame_detail.values()):
        print_stats_table("Detailed frame samples", [(field, frame_detail[field]) for field in FRAME_MS_FIELDS])
    if any(frame_basic.values()):
        print_stats_table("Basic frame samples", list(frame_basic.items()))
    if water_particles:
        print_stats_table(
            "Water samples",
            [("particles", water_particles), ("avg_substep", water_avg_substep)]
            + [(field, water[field]) for field in WATER_MS_FIELDS if water[field]],
        )
    print_stats_table(
        "Terrain / water terrain events",
        [
            ("terrain_deferred_rebuild", deferred_rebuild),
            ("terrain_sync_visible_rebuild", sync_visible_rebuild),
            ("terrain_deferred_surface_total", deferred_surface_total),
            ("terrain_deferred_contree_total", deferred_contree_total),
            ("terrain_deferred_scene_total", deferred_scene_total),
            ("terrain_deferred_scene_finish", deferred_scene_finish),
            ("terrain_deferred_preserve_flora_edit", deferred_preserve_flora),
            ("surface_build_total", surface_build_total),
            ("surface_build_fence_latency", surface_build_fence_latency),
            ("surface_build_flora", surface_build_flora),
            ("contree_rebuild_total", contree_rebuild_total),
            ("contree_rebuild_fence_latency", contree_rebuild_fence_latency),
            ("contree_rebuild_size", contree_rebuild_size),
            ("contree_rebuild_confirm", contree_rebuild_confirm),
            ("terrain_sdf_source_apply", source_refresh),
            ("terrain_sdf_source_gpu_sample_total", source_gpu_sample_total),
            ("terrain_sdf_source_fence_latency", source_fence_latency),
            ("terrain_sdf_source_gpu_submit", source_gpu_submit),
            ("terrain_sdf_source_gpu_readback", source_gpu_readback),
            ("terrain_sdf_collider_build", collider_build),
            ("water_cache_worker", cache_worker),
            ("water_cache_apply", cache_apply),
            ("water_cache_discard_worker", cache_discard_worker),
        ],
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", nargs="?", type=Path, help="log path; defaults to latest target/re-flora-logs/*.log")
    args = parser.parse_args()
    parse_log(args.log or latest_log())


if __name__ == "__main__":
    main()
