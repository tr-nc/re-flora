#!/usr/bin/env python3
"""Fail-closed per-process vegetation comparison; no startup/phase-mixing samples."""
import argparse
import collections
import json
import math
import re
import statistics
from pathlib import Path


def analyze(path):
    text = Path(path).read_text()
    return analyze_text(text, path)


def analyze_text(text, path="fixture"):
    assert not re.search(r"\bERROR\b|panicked|VUID-", text), f"errors in {path}"
    assert "Application exited successfully" in text, f"incomplete run {path}"
    markers = re.findall(r"\[VEGETATION_RESPONSE\]\[BENCH\] phase=(\w+) app_frame=(\d+).*?flora=(\[[^\]]+\]) leaves=(\d+) apples=(\d+)", text)
    assert [m[0] for m in markers] == ["sample", "complete"], markers
    assert markers[0][2:] == markers[1][2:], f"changing workload: {markers}"
    start, end = (int(m[1]) for m in markers)
    assert end - start == 2000, (start, end)
    # Latest completed GPU query is at most a few in-flight frames behind CPU.
    # Explicitly exclude first 8 samples, never silently discard slow samples.
    start += 8
    metrics = collections.defaultdict(list)
    frames = collections.defaultdict(set)
    for kind, frame, rest in re.findall(r"\[PERF\]\[(GPU|CPU)_FRAME_SCOPE\] frame (\d+) (.*)", text):
        frame = int(frame)
        if not start <= frame < end:
            continue
        assert frame not in frames[kind], f"duplicate {kind} frame {frame}"
        frames[kind].add(frame)
        if kind == "GPU":
            assert re.search(r"dropped=0\b", rest), f"dropped scopes: {rest}"
        for key, value in re.findall(r"([\w.]+)=([^\s=]+)us", rest):
            number = float(value)
            assert math.isfinite(number) and number >= 0
            metrics[f"{kind}.{key}"].append(number)
    for kind in ["GPU", "CPU"]:
        assert frames[kind] == set(range(start, end)), (path, kind, len(frames[kind]), end - start)
    wanted = ["GPU.frame.render", "GPU.vegetation_response.pass", "GPU.graphics.flora", "GPU.graphics.leaves", "GPU.graphics.apples", "GPU.graphics.leaf_lighting_cache", "CPU.frame.cpu_total", "CPU.render.record", "CPU.render.acquire"]
    stats = {}
    for key in wanted:
        values = metrics.get(key, [])
        assert len(values) == end - start, f"missing or invalid {key} samples in {path}"
        ordered = sorted(values)
        stats[key] = dict(n=len(values), median=statistics.median(values), mean=statistics.mean(values), p95=ordered[math.ceil(0.95 * len(values)) - 1], minimum=ordered[0], maximum=ordered[-1])
    allocations = {}
    mode = None
    for m, slot, capacity, size in re.findall(r"\[VEGETATION_RESPONSE\]\[MEMORY\] mode=(\w+) frame_slot=(\d+) capacity=(\d+) buffer_bytes=(\d+)", text):
        mode = m
        allocations[int(slot)] = int(size)
    assert mode in ("legacy", "surface", "all"), f"missing allocation mode in {path}"
    return dict(path=str(path), mode=mode, extent=re.search(r"Hidden window render extent is (\d+x\d+)", text).group(1), flora=json.loads(markers[0][2]), leaves=int(markers[0][3]), apples=int(markers[0][4]), response_buffer_bytes=sum(allocations.values()), frames=end - start, metrics_us=stats)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("logs", nargs="+")
    args = parser.parse_args()
    runs = [analyze(path) for path in args.logs]
    for key in ["extent", "flora", "leaves", "apples", "frames"]:
        assert all(run[key] == runs[0][key] for run in runs), f"incomparable {key}"
    print(json.dumps(runs, indent=2))


if __name__ == "__main__":
    main()
