#!/usr/bin/env python3
"""Summarize Verdarium perf logs into compact tables.

Default output is Markdown for easy pasting into docs or issue comments. Use
``--format json`` or ``--format csv`` when feeding the summary into another tool.
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import statistics
import sys
from collections import OrderedDict
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterable, Optional, TextIO


ROOT = Path(__file__).resolve().parents[1]
LOG_DIR = ROOT / "target" / "verdarium-logs"
LATEST_POINTER_FILE = "latest-run-log.txt"

FRAME_DETAIL_RE = re.compile(r"\[PERF\]\[FRAME\] frame (\d+) ")
FRAME_RE = re.compile(
    r"\[PERF\] frame (\d+) total ([0-9.]+)ms egui ([0-9.]+)ms gpu\+present ([0-9.]+)ms"
)
WATER_RE = re.compile(
    r"\[PERF\]\[WATER\] particles (\d+) .*? substeps (\d+) total ([0-9.]+)ms avg ([0-9.]+)ms/substep"
)
WATER_THREAD_RE = re.compile(r"\[PERF\]\[WATER_THREAD\]")
PARTICLES_RE = re.compile(r"\[PERF\]\[PARTICLES\]")
GPU_FRAME_SCOPE_RE = re.compile(r"\[PERF\]\[GPU_FRAME_SCOPE\]")
GPU_JOB_SCOPE_RE = re.compile(r"\[PERF\]\[GPU_JOB_SCOPE\]")
DEFERRED_REBUILD_RE = re.compile(r"\[PERF\]\[DEFERRED_REBUILD\].* total ([0-9.]+)ms")
SYNC_VISIBLE_REBUILD_RE = re.compile(r"\[PERF\]\[SYNC_VISIBLE_REBUILD\]")
DEFERRED_REBUILD_PHASE_RE = re.compile(r"\[PERF\]\[DEFERRED_REBUILD_PHASE\]")
SURFACE_BUILD_RE = re.compile(r"\[PERF\]\[SURFACE_BUILD\]")
CONTREE_REBUILD_RE = re.compile(r"\[QUEUE\]\[CONTREE_REBUILD\]")
SOURCE_REFRESH_RE = re.compile(r"refreshed GPU solid source")
COLLIDER_BUILD_RE = re.compile(r"built collider chunk .* build_ms=([0-9.]+)")
CACHE_APPLY_RE = re.compile(r"applied worker grid cache region .* worker_ms=([0-9.]+) apply_ms=([0-9.]+)")
CACHE_DISCARD_RE = re.compile(r"discarded stale worker grid cache region .* worker_ms=([0-9.]+)")

KEY_VALUE_WITH_UNIT_RE = re.compile(r"\b([A-Za-z0-9_.:-]+)=([0-9.]+)(us|ms)\b")
GPU_JOB_NAME_RE = re.compile(r"\bname=([^\s]+)")
GPU_JOB_DURATION_RE = re.compile(r"\bduration=([0-9.]+)us\b")
NUMBER_TOKEN = r"[-+]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][-+]?\d+)?"

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
    "water_handoff",
    "particles",
    "tracked_cpu",
    "untracked_cpu",
]

FRAME_QUEUE_COUNT_FIELDS = [
    "deferred_pending",
    "deferred_active",
    "source_pending",
    "source_active",
    "collider_pending",
    "collider_active",
    "cache_pending",
    "cache_active",
]

FRAME_QUEUE_BOOL_FIELDS = [
    "deferred_inflight",
    "collider_inflight",
    "cache_inflight",
]

WATER_MS_FIELDS = [
    "total",
    "repair",
    "clear",
    "p2g",
    "grid",
    "grid_update",
    "g2p",
    "g2p_gather",
    "g2p_box",
    "g2p_terrain",
    "g2p_repair",
    "diagnostics",
    "residual",
    "shadow_measure",
]

PARTICLE_MS_FIELDS = [
    "total",
    "setup",
    "emit",
    "sim",
    "collect",
    "plan",
    "snapshot",
    "upload",
]

PARTICLE_COUNT_FIELDS = ["alive", "snapshots", "water_debug", "butterflies", "leaves"]

WATER_DIAGNOSTIC_FIELDS = [
    ("p2g_density_corr/substep", "p2g_density_corr_per_substep"),
    ("p2g_density_corr_factor_avg", "p2g_density_corr_factor_avg"),
    ("p2g_density_corr_factor_max", "p2g_density_corr_factor_max"),
    ("terrain_cache_skips/substep", "terrain_cache_skips_per_substep"),
    ("terrain_cache_projections/substep", "terrain_cache_projections_per_substep"),
    ("terrain_exact_fallbacks/substep", "terrain_exact_fallbacks_per_substep"),
    ("terrain_exact_checks/substep", "terrain_exact_checks_per_substep"),
    ("terrain_exact_corrections/substep", "terrain_exact_corrections_per_substep"),
    ("terrain_shadow_samples/substep", "terrain_shadow_samples_per_substep"),
    ("terrain_shadow_false_skips", "terrain_shadow_false_skips"),
    ("terrain_shadow_sdf_err_avg", "terrain_shadow_sdf_err_avg"),
    ("terrain_shadow_sdf_err_max", "terrain_shadow_sdf_err_max"),
    ("active_nodes/substep", "active_nodes_per_substep"),
    ("terrain_sdf_min", "terrain_sdf_min"),
    ("penetrating", "terrain_penetrating"),
    ("no_sdf", "terrain_no_sdf"),
]

WATER_THREAD_MS_FIELDS = ["command_drain", "publish", "publish_lock"]

WATER_THREAD_VALUE_FIELDS = [
    "seconds",
    "particles",
    "ticks",
    "active_ticks",
    "idle_ticks",
    "commands",
    "commands_per_tick",
    "max_commands_per_tick",
    "maxed_command_ticks",
    "publish_count",
    "publish_particles",
    "publish_particles_per_publish",
    "snapshot_bucket_count",
]


@dataclass
class MetricGroup:
    title: str
    unit: str
    metrics: OrderedDict[str, list[float]] = field(default_factory=OrderedDict)

    def add(self, metric: str, value: float) -> None:
        self.metrics.setdefault(metric, []).append(value)

    def has_samples(self) -> bool:
        return any(self.metrics.values())


@dataclass
class PerfSummary:
    source: str
    groups: OrderedDict[str, MetricGroup] = field(default_factory=OrderedDict)

    def add(self, group_key: str, title: str, unit: str, metric: str, value: float) -> None:
        group = self.groups.setdefault(group_key, MetricGroup(title=title, unit=unit))
        group.add(metric, value)

    def nonempty_groups(self) -> Iterable[MetricGroup]:
        return (group for group in self.groups.values() if group.has_samples())


def latest_log(log_dir: Path = LOG_DIR) -> Path:
    pointer_path = log_dir / LATEST_POINTER_FILE
    if pointer_path.is_file():
        pointed_path = Path(pointer_path.read_text().strip())
        if pointed_path.is_file():
            return pointed_path

    logs = sorted(log_dir.glob("*.log"), key=lambda path: path.stat().st_mtime)
    if not logs:
        raise SystemExit(f"no logs found under {log_dir}")
    return logs[-1]


def extract_ms(line: str, field_name: str) -> Optional[float]:
    match = re.search(rf"\b{re.escape(field_name)} ([0-9.]+)ms", line)
    if not match:
        return None
    return float(match.group(1))


def extract_eq_ms(line: str, field_name: str) -> Optional[float]:
    match = re.search(rf"\b{re.escape(field_name)}=([0-9.]+)ms", line)
    if not match:
        return None
    return float(match.group(1))


def extract_eq_count(line: str, field_name: str) -> Optional[float]:
    match = re.search(rf"\b{re.escape(field_name)}=(\d+)\b", line)
    if not match:
        return None
    return float(match.group(1))


def extract_eq_bool(line: str, field_name: str) -> Optional[float]:
    match = re.search(rf"\b{re.escape(field_name)}=(true|false)\b", line)
    if not match:
        return None
    return 1.0 if match.group(1) == "true" else 0.0


def extract_eq_number(line: str, field_name: str) -> Optional[float]:
    match = re.search(rf"\b{re.escape(field_name)}=({NUMBER_TOKEN})\b", line)
    if not match:
        return None
    return float(match.group(1))


def extract_labeled_number(line: str, field_name: str) -> Optional[float]:
    match = re.search(rf"(?<!\S){re.escape(field_name)}\s+({NUMBER_TOKEN})(?=\s|$)", line)
    if not match:
        return None
    return float(match.group(1))


def us_to_ms(value_us: float) -> float:
    return value_us / 1000.0


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


def parse_key_value_durations_ms(line: str) -> Iterable[tuple[str, float]]:
    for metric, value, unit in KEY_VALUE_WITH_UNIT_RE.findall(line):
        value_float = float(value)
        yield metric, us_to_ms(value_float) if unit == "us" else value_float


def parse_log_lines(lines: Iterable[str], source: str = "<memory>") -> PerfSummary:
    summary = PerfSummary(source=source)

    for line in lines:
        if FRAME_DETAIL_RE.search(line):
            for field_name in FRAME_MS_FIELDS:
                value = extract_ms(line, field_name)
                if value is not None:
                    summary.add("frame_detail", "Detailed frame samples", "ms", field_name, value)
            for field_name in FRAME_QUEUE_COUNT_FIELDS:
                value = extract_eq_count(line, field_name)
                if value is not None:
                    summary.add("frame_queues", "Frame queue samples", "count", field_name, value)
            for field_name in FRAME_QUEUE_BOOL_FIELDS:
                value = extract_eq_bool(line, field_name)
                if value is not None:
                    summary.add("frame_queues", "Frame queue samples", "count", field_name, value)
            continue

        if match := FRAME_RE.search(line):
            total = float(match.group(2))
            egui = float(match.group(3))
            gpu = float(match.group(4))
            summary.add("frame_basic", "Basic frame samples", "ms", "total", total)
            summary.add("frame_basic", "Basic frame samples", "ms", "egui", egui)
            summary.add("frame_basic", "Basic frame samples", "ms", "gpu_present", gpu)
            summary.add("frame_basic", "Basic frame samples", "ms", "cpu_other", max(0.0, total - egui - gpu))
            continue

        if match := WATER_RE.search(line):
            summary.add("water_counts", "Water counts", "count", "particles", float(match.group(1)))
            summary.add("water_counts", "Water counts", "count", "substeps", float(match.group(2)))
            summary.add("water", "Water timings", "ms", "total", float(match.group(3)))
            summary.add("water", "Water timings", "ms", "avg_substep", float(match.group(4)))
            for field_name in WATER_MS_FIELDS:
                if field_name == "total":
                    continue
                value = extract_ms(line, field_name)
                if value is not None:
                    summary.add("water", "Water timings", "ms", field_name, value)
            for field_name, metric in WATER_DIAGNOSTIC_FIELDS:
                value = extract_labeled_number(line, field_name)
                if value is not None:
                    summary.add("water_diagnostics", "Water diagnostics", "value", metric, value)
            continue

        if WATER_THREAD_RE.search(line):
            if (value := extract_eq_bool(line, "enabled")) is not None:
                summary.add("water_thread_values", "Water thread samples", "value", "enabled", value)
            for field_name in WATER_THREAD_VALUE_FIELDS:
                value = extract_eq_number(line, field_name)
                if value is not None:
                    summary.add("water_thread_values", "Water thread samples", "value", field_name, value)
            for field_name in WATER_THREAD_MS_FIELDS:
                value = extract_eq_ms(line, field_name)
                if value is not None:
                    summary.add("water_thread", "Water thread timings", "ms", field_name, value)
            continue

        if PARTICLES_RE.search(line):
            for field_name in PARTICLE_MS_FIELDS:
                value = extract_eq_ms(line, field_name)
                if value is not None:
                    summary.add("particles", "Particle timings", "ms", field_name, value)
            for field_name in PARTICLE_COUNT_FIELDS:
                value = extract_eq_count(line, field_name)
                if value is not None:
                    summary.add("particle_counts", "Particle counts", "count", field_name, value)
            continue

        if GPU_FRAME_SCOPE_RE.search(line):
            for metric, value_ms in parse_key_value_durations_ms(line):
                summary.add("gpu_frame_scope", "GPU frame scopes", "ms", metric, value_ms)
            continue

        if GPU_JOB_SCOPE_RE.search(line):
            name = GPU_JOB_NAME_RE.search(line)
            duration = GPU_JOB_DURATION_RE.search(line)
            if name and duration:
                summary.add(
                    "gpu_job_scope",
                    "GPU job scopes",
                    "ms",
                    name.group(1),
                    us_to_ms(float(duration.group(1))),
                )
            continue

        if SYNC_VISIBLE_REBUILD_RE.search(line):
            if (value := extract_ms(line, "total")) is not None:
                summary.add("terrain_events", "Terrain / water terrain events", "ms", "terrain_sync_visible_rebuild", value)
            continue

        if match := DEFERRED_REBUILD_RE.search(line):
            summary.add("terrain_events", "Terrain / water terrain events", "ms", "terrain_deferred_rebuild", float(match.group(1)))
            for field_name, metric in [
                ("surface_total", "terrain_deferred_surface_total"),
                ("contree_total", "terrain_deferred_contree_total"),
                ("scene_total", "terrain_deferred_scene_total"),
                ("scene_finish", "terrain_deferred_scene_finish"),
            ]:
                if (value := extract_ms(line, field_name)) is not None:
                    summary.add("terrain_events", "Terrain / water terrain events", "ms", metric, value)
            continue

        if DEFERRED_REBUILD_PHASE_RE.search(line):
            value = extract_ms(line, "preserve_flora")
            if value is not None and value > 0.0:
                summary.add("terrain_events", "Terrain / water terrain events", "ms", "terrain_deferred_preserve_flora_edit", value)
            continue

        if SURFACE_BUILD_RE.search(line):
            for field_name, metric in [
                ("total", "surface_build_total"),
                ("fence_latency", "surface_build_fence_latency"),
                ("flora", "surface_build_flora"),
            ]:
                if (value := extract_ms(line, field_name)) is not None:
                    summary.add("terrain_events", "Terrain / water terrain events", "ms", metric, value)
            continue

        if CONTREE_REBUILD_RE.search(line):
            for field_name, metric in [
                ("total_ms", "contree_rebuild_total"),
                ("fence_latency_ms", "contree_rebuild_fence_latency"),
                ("size_ms", "contree_rebuild_size"),
                ("confirm_ms", "contree_rebuild_confirm"),
            ]:
                if (value := extract_eq_ms(line, field_name)) is not None:
                    summary.add("terrain_events", "Terrain / water terrain events", "ms", metric, value)
            continue

        if SOURCE_REFRESH_RE.search(line):
            for field_name, metric in [
                ("total", "terrain_sdf_source_apply"),
                ("gpu_sample_total", "terrain_sdf_source_gpu_sample_total"),
                ("fence_latency", "terrain_sdf_source_fence_latency"),
                ("gpu_submit", "terrain_sdf_source_gpu_submit"),
                ("gpu_readback", "terrain_sdf_source_gpu_readback"),
            ]:
                if (value := extract_eq_ms(line, field_name)) is not None:
                    summary.add("terrain_events", "Terrain / water terrain events", "ms", metric, value)
            continue

        if match := COLLIDER_BUILD_RE.search(line):
            summary.add("terrain_events", "Terrain / water terrain events", "ms", "terrain_sdf_collider_build", float(match.group(1)))
            continue

        if match := CACHE_APPLY_RE.search(line):
            summary.add("terrain_events", "Terrain / water terrain events", "ms", "water_cache_worker", float(match.group(1)))
            summary.add("terrain_events", "Terrain / water terrain events", "ms", "water_cache_apply", float(match.group(2)))
            continue

        if match := CACHE_DISCARD_RE.search(line):
            summary.add("terrain_events", "Terrain / water terrain events", "ms", "water_cache_discard_worker", float(match.group(1)))
            continue

    return summary


def parse_log(path: Path) -> PerfSummary:
    return parse_log_lines(path.read_text(errors="replace").splitlines(), source=str(path))


def print_stats_table(group: MetricGroup, out: TextIO = sys.stdout) -> None:
    print(f"\n## {group.title}\n", file=out)
    unit_suffix = f" {group.unit}" if group.unit else ""
    print(
        f"| metric | n | sum{unit_suffix} | mean{unit_suffix} | median{unit_suffix} | p95{unit_suffix} | max{unit_suffix} |",
        file=out,
    )
    print("|---|---:|---:|---:|---:|---:|---:|", file=out)
    for metric, values in group.metrics.items():
        stats = summarize(values)
        if stats["n"] == 0:
            continue
        print(
            f"| {metric} | {stats['n']:.0f} | {stats['sum']:.2f} | {stats['mean']:.2f} | "
            f"{stats['median']:.2f} | {stats['p95']:.2f} | {stats['max']:.2f} |",
            file=out,
        )


def write_markdown(summary: PerfSummary, out: TextIO = sys.stdout) -> None:
    print(f"# Perf log summary\n\n`{summary.source}`", file=out)
    for group in summary.nonempty_groups():
        print_stats_table(group, out)


def stats_rows(summary: PerfSummary) -> Iterable[dict[str, object]]:
    for group in summary.nonempty_groups():
        for metric, values in group.metrics.items():
            stats = summarize(values)
            if stats["n"] == 0:
                continue
            yield {
                "group": group.title,
                "metric": metric,
                "unit": group.unit,
                **stats,
            }


def write_json(summary: PerfSummary, out: TextIO = sys.stdout) -> None:
    payload = {
        "source": summary.source,
        "metrics": list(stats_rows(summary)),
    }
    json.dump(payload, out, indent=2, sort_keys=True)
    print(file=out)


def write_csv(summary: PerfSummary, out: TextIO = sys.stdout) -> None:
    fieldnames = ["group", "metric", "unit", "n", "sum", "mean", "median", "p95", "max"]
    writer = csv.DictWriter(out, fieldnames=fieldnames)
    writer.writeheader()
    for row in stats_rows(summary):
        writer.writerow(row)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", nargs="?", type=Path, help="log path; defaults to latest target/verdarium-logs/*.log")
    parser.add_argument(
        "--format",
        choices=("markdown", "json", "csv"),
        default="markdown",
        help="summary output format (default: markdown)",
    )
    args = parser.parse_args()

    summary = parse_log(args.log or latest_log())
    if args.format == "markdown":
        write_markdown(summary)
    elif args.format == "json":
        write_json(summary)
    elif args.format == "csv":
        write_csv(summary)


if __name__ == "__main__":
    main()
