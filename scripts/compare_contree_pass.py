#!/usr/bin/env python3
"""Compare one contree GPU pass across matched tree-benchmark logs."""

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
    r"\[PERF\]\[CONTREE_PASS_TIMING\] chunk (UVec3\([^)]*\)) "
    r"pass_total=([0-9]+(?:\.[0-9]+)?)ms (.*)"
)
PASS_RE = re.compile(r"(\w+)=([0-9]+(?:\.[0-9]+)?)ms")
BUILD_RE = re.compile(
    r"\[QUEUE\]\[CONTREE_REBUILD\] chunk (UVec3\([^)]*\)).*"
    r"node_bytes=(\d+) leaf_bytes=(\d+)"
)


@dataclass(frozen=True)
class ContreeSample:
    chunk: str
    node_bytes: int
    leaf_bytes: int
    duration_ms: float

    @property
    def workload(self) -> tuple[str, int, int]:
        return self.chunk, self.node_bytes, self.leaf_bytes


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
    parser.add_argument("baseline", type=Path, help="baseline tree-benchmark run log")
    parser.add_argument("candidate", type=Path, help="candidate tree-benchmark run log")
    parser.add_argument("--pass", dest="pass_name", default="leaf_write", help="pass label")
    parser.add_argument(
        "--min-leaf-bytes",
        type=int,
        default=500_000,
        help="discard lighter contrees (default: 500000)",
    )
    return parser.parse_args()


def read_samples(path: Path, pass_name: str) -> list[ContreeSample]:
    pending: defaultdict[str, deque[float]] = defaultdict(deque)
    samples: list[ContreeSample] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if timing_match := TIMING_RE.search(line):
            pass_times = dict(PASS_RE.findall(timing_match.group(3)))
            pass_times["pass_total"] = timing_match.group(2)
            if pass_name in pass_times:
                pending[timing_match.group(1)].append(float(pass_times[pass_name]))
            continue

        build_match = BUILD_RE.search(line)
        if build_match is None:
            continue

        chunk = build_match.group(1)
        if not pending[chunk]:
            raise ValueError(
                f"contree build for {chunk} has no preceding {pass_name!r} timing in {path}"
            )
        samples.append(
            ContreeSample(
                chunk=chunk,
                node_bytes=int(build_match.group(2)),
                leaf_bytes=int(build_match.group(3)),
                duration_ms=pending[chunk].popleft(),
            )
        )

    if not samples:
        raise ValueError(f"no {pass_name!r} contree samples in {path}")
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


def summarize(samples: list[ContreeSample]) -> Summary:
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
        f"mean={summary.mean_ms * 1000.0:.2f}us "
        f"median={summary.median_ms * 1000.0:.2f}us "
        f"p95={summary.p95_ms * 1000.0:.2f}us "
        f"range={summary.min_ms * 1000.0:.2f}..{summary.max_ms * 1000.0:.2f}us"
    )


def main() -> int:
    args = parse_args()
    try:
        baseline_all = read_samples(args.baseline, args.pass_name)
        candidate_all = read_samples(args.candidate, args.pass_name)
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1

    baseline = [
        sample for sample in baseline_all if sample.leaf_bytes >= args.min_leaf_bytes
    ]
    candidate = [
        sample for sample in candidate_all if sample.leaf_bytes >= args.min_leaf_bytes
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
    print(
        f"pass={args.pass_name} min_leaf_bytes={args.min_leaf_bytes} "
        "workloads_match=true"
    )
    print_summary("baseline", args.baseline, baseline_summary)
    print_summary("candidate", args.candidate, candidate_summary)
    print(
        "candidate_vs_baseline "
        f"mean={(candidate_summary.mean_ms - baseline_summary.mean_ms) * 1000.0:+.2f}us "
        f"({percentage_delta(baseline_summary.mean_ms, candidate_summary.mean_ms):+.2f}%) "
        f"median={(candidate_summary.median_ms - baseline_summary.median_ms) * 1000.0:+.2f}us "
        f"({percentage_delta(baseline_summary.median_ms, candidate_summary.median_ms):+.2f}%) "
        f"p95={(candidate_summary.p95_ms - baseline_summary.p95_ms) * 1000.0:+.2f}us "
        f"({percentage_delta(baseline_summary.p95_ms, candidate_summary.p95_ms):+.2f}%)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
