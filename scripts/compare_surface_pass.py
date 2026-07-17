#!/usr/bin/env python3
"""Compare make_surface_sparse GPU timings across matched tree-benchmark logs."""

from __future__ import annotations

import argparse
import math
import re
import statistics
import sys
from collections import defaultdict, deque
from dataclasses import dataclass
from pathlib import Path

TIMING_RE = re.compile(
    r"\[PERF\]\[SURFACE_BUILD_PASS_TIMING\] chunk (UVec3\([^)]*\)).*"
    r"make_surface_sparse=([0-9]+(?:\.[0-9]+)?)ms"
)
BUILD_RE = re.compile(
    r"\[PERF\]\[SURFACE_BUILD\] chunk (UVec3\([^)]*\)).*"
    r"active_voxels (\d+) active_bricks (\d+) solid_workgroups (\d+)"
)


@dataclass(frozen=True)
class SurfaceSample:
    chunk: str
    active_voxels: int
    active_bricks: int
    solid_workgroups: int
    duration_ms: float

    @property
    def workload(self) -> tuple[str, int, int, int]:
        return (
            self.chunk,
            self.active_voxels,
            self.active_bricks,
            self.solid_workgroups,
        )


@dataclass(frozen=True)
class Summary:
    count: int
    mean_ms: float
    median_ms: float
    p95_ms: float
    min_ms: float
    max_ms: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path, help="GLSL tree-benchmark run log")
    parser.add_argument("candidate", type=Path, help="Slang tree-benchmark run log")
    parser.add_argument(
        "--min-solid-workgroups",
        type=int,
        default=10_000,
        help="discard lighter dispatches (default: 10000)",
    )
    return parser.parse_args()


def read_samples(path: Path) -> list[SurfaceSample]:
    pending: defaultdict[str, deque[float]] = defaultdict(deque)
    samples: list[SurfaceSample] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if timing_match := TIMING_RE.search(line):
            pending[timing_match.group(1)].append(float(timing_match.group(2)))
            continue

        build_match = BUILD_RE.search(line)
        if build_match is None:
            continue

        chunk = build_match.group(1)
        if not pending[chunk]:
            raise ValueError(f"surface build for {chunk} has no preceding timing in {path}")
        samples.append(
            SurfaceSample(
                chunk=chunk,
                active_voxels=int(build_match.group(2)),
                active_bricks=int(build_match.group(3)),
                solid_workgroups=int(build_match.group(4)),
                duration_ms=pending[chunk].popleft(),
            )
        )

    if not samples:
        raise ValueError(f"no surface pass samples in {path}")
    return samples


def percentile(values: list[float], percentile_value: float) -> float:
    ordered = sorted(values)
    rank = (len(ordered) - 1) * percentile_value
    low = math.floor(rank)
    high = math.ceil(rank)
    if low == high:
        return ordered[low]
    fraction = rank - low
    return ordered[low] * (1.0 - fraction) + ordered[high] * fraction


def summarize(samples: list[SurfaceSample]) -> Summary:
    values = [sample.duration_ms for sample in samples]
    return Summary(
        count=len(values),
        mean_ms=statistics.fmean(values),
        median_ms=statistics.median(values),
        p95_ms=percentile(values, 0.95),
        min_ms=min(values),
        max_ms=max(values),
    )


def percentage_delta(baseline: float, candidate: float) -> float:
    return (candidate - baseline) / baseline * 100.0


def print_summary(label: str, path: Path, summary: Summary) -> None:
    print(
        f"{label} path={path} samples={summary.count} "
        f"mean={summary.mean_ms:.3f}ms median={summary.median_ms:.3f}ms "
        f"p95={summary.p95_ms:.3f}ms "
        f"range={summary.min_ms:.3f}..{summary.max_ms:.3f}ms"
    )


def main() -> int:
    args = parse_args()
    try:
        baseline_all = read_samples(args.baseline)
        candidate_all = read_samples(args.candidate)
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1

    baseline = [
        sample
        for sample in baseline_all
        if sample.solid_workgroups >= args.min_solid_workgroups
    ]
    candidate = [
        sample
        for sample in candidate_all
        if sample.solid_workgroups >= args.min_solid_workgroups
    ]
    if not baseline or not candidate:
        print("workload filter removed every sample", file=sys.stderr)
        return 1

    baseline_workloads = [sample.workload for sample in baseline]
    candidate_workloads = [sample.workload for sample in candidate]
    if baseline_workloads != candidate_workloads:
        mismatch_index = next(
            (
                index
                for index, pair in enumerate(zip(baseline_workloads, candidate_workloads))
                if pair[0] != pair[1]
            ),
            min(len(baseline_workloads), len(candidate_workloads)),
        )
        print(
            f"workloads do not match at filtered sample {mismatch_index}: "
            f"baseline_count={len(baseline_workloads)} "
            f"candidate_count={len(candidate_workloads)}",
            file=sys.stderr,
        )
        return 1

    baseline_summary = summarize(baseline)
    candidate_summary = summarize(candidate)
    print(f"min_solid_workgroups={args.min_solid_workgroups} workloads_match=true")
    print_summary("baseline", args.baseline, baseline_summary)
    print_summary("candidate", args.candidate, candidate_summary)
    print(
        "candidate_vs_baseline "
        f"mean={candidate_summary.mean_ms - baseline_summary.mean_ms:+.3f}ms "
        f"({percentage_delta(baseline_summary.mean_ms, candidate_summary.mean_ms):+.2f}%) "
        f"median={candidate_summary.median_ms - baseline_summary.median_ms:+.3f}ms "
        f"({percentage_delta(baseline_summary.median_ms, candidate_summary.median_ms):+.2f}%) "
        f"p95={candidate_summary.p95_ms - baseline_summary.p95_ms:+.3f}ms "
        f"({percentage_delta(baseline_summary.p95_ms, candidate_summary.p95_ms):+.2f}%)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
