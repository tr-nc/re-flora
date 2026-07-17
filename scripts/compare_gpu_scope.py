#!/usr/bin/env python3
"""Compare one Vulkan timestamp scope across two re-flora run logs."""

from __future__ import annotations

import argparse
import math
import re
import statistics
import sys
from dataclasses import dataclass
from pathlib import Path

FRAME_RE = re.compile(r"\[PERF\]\[GPU_FRAME_SCOPE\] frame (\d+)")


@dataclass(frozen=True)
class Summary:
    samples: int
    mean_us: float
    median_us: float
    p95_us: float
    min_us: float
    max_us: float


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=Path, help="baseline run log")
    parser.add_argument("candidate", type=Path, help="candidate run log")
    parser.add_argument("--scope", default="post_processing.pass", help="GPU scope name")
    parser.add_argument(
        "--min-frame",
        type=int,
        default=120,
        help="discard samples before this frame (default: 120)",
    )
    return parser.parse_args()


def read_scope_samples(path: Path, scope: str, min_frame: int) -> list[float]:
    scope_re = re.compile(rf"(?:^|\s){re.escape(scope)}=([0-9]+(?:\.[0-9]+)?)us(?:\s|$)")
    samples: list[float] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        frame_match = FRAME_RE.search(line)
        if frame_match is None or int(frame_match.group(1)) < min_frame:
            continue
        scope_match = scope_re.search(line)
        if scope_match is not None:
            samples.append(float(scope_match.group(1)))
    if not samples:
        raise ValueError(f"no {scope!r} samples at frame >= {min_frame} in {path}")
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


def summarize(samples: list[float]) -> Summary:
    return Summary(
        samples=len(samples),
        mean_us=statistics.fmean(samples),
        median_us=statistics.median(samples),
        p95_us=percentile(samples, 0.95),
        min_us=min(samples),
        max_us=max(samples),
    )


def percentage_delta(baseline: float, candidate: float) -> float:
    return (candidate - baseline) / baseline * 100.0


def print_summary(label: str, path: Path, summary: Summary) -> None:
    print(
        f"{label} path={path} samples={summary.samples} "
        f"mean={summary.mean_us:.2f}us median={summary.median_us:.2f}us "
        f"p95={summary.p95_us:.2f}us range={summary.min_us:.2f}..{summary.max_us:.2f}us"
    )


def main() -> int:
    args = parse_args()
    try:
        baseline = summarize(read_scope_samples(args.baseline, args.scope, args.min_frame))
        candidate = summarize(read_scope_samples(args.candidate, args.scope, args.min_frame))
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1

    print(f"scope={args.scope} min_frame={args.min_frame}")
    print_summary("baseline", args.baseline, baseline)
    print_summary("candidate", args.candidate, candidate)
    print(
        "candidate_vs_baseline "
        f"mean={candidate.mean_us - baseline.mean_us:+.2f}us "
        f"({percentage_delta(baseline.mean_us, candidate.mean_us):+.2f}%) "
        f"median={candidate.median_us - baseline.median_us:+.2f}us "
        f"({percentage_delta(baseline.median_us, candidate.median_us):+.2f}%)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
