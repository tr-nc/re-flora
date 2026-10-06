#!/usr/bin/env python3
"""Diagnose persistent tree-count frame cliffs using the real Release app.

Build with cargo build --release first. Opt-in replay changes no rendering rules.
Exit 1 = captured a >=80 ms median / >=3x adjacent-count slowdown; 0 = not
captured; 2 = invalid evidence. These are symptom thresholds, not release budgets.
"""

import argparse
import fcntl
import hashlib
import json
import os
import re
import statistics
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PHASE = re.compile(r"\[DEBUG-tree-scale\] phase trees=(\d+)")
SAMPLE = re.compile(
    r"\[DEBUG-tree-scale\] sample trees=(\d+) frame=(\d+) "
    r"total_ms=([\d.]+) gpu_present_ms=([\d.]+) untracked_cpu_ms=([\d.]+)"
)
GPU = re.compile(r"([\w.]+)=(\d+)us")


def summarize(log, count):
    phases = {i: {"samples": [], "errors": [], "gpu_scopes_us": {}} for i in range(1, count + 1)}
    current = 0
    for line in log.splitlines():
        phase = PHASE.search(line)
        if phase:
            current = int(phase[1])
        sample = SAMPLE.search(line)
        if sample:
            phases[int(sample[1])]["samples"].append({
                "frame": int(sample[2]), "total_ms": float(sample[3]),
                "gpu_present_ms": float(sample[4]), "untracked_cpu_ms": float(sample[5]),
            })
        if current and "[TREE][RASTER_STATIC] preparation failed" in line:
            phases[current]["errors"].append(line)
        if current and "[PERF][GPU_FRAME_SCOPE]" in line and len(phases[current]["samples"]) > 2:
            for name, duration in GPU.findall(line):
                phases[current]["gpu_scopes_us"].setdefault(name, []).append(int(duration))
    for tree_count, phase in phases.items():
        if len(phase["samples"]) < 3:
            raise ValueError(f"tree count {tree_count}: fewer than three settled frames")
        for metric in ["total_ms", "gpu_present_ms", "untracked_cpu_ms"]:
            values = [sample[metric] for sample in phase["samples"]]
            phase[f"median_{metric}"] = statistics.median(values)
            phase[f"max_{metric}"] = max(values)
        phase["gpu_scope_medians_us"] = {
            name: statistics.median(values) for name, values in phase["gpu_scopes_us"].items()
        }
        prior = phases.get(tree_count - 1)
        phase["frame_cliff"] = bool(
            prior and phase["median_total_ms"] >= 80
            and phase["median_total_ms"] >= 3 * prior["median_total_ms"]
        )
    if f"[DEBUG-tree-scale] completed trees={count}" not in log:
        raise ValueError("tree replay did not complete")
    if "Application exited successfully" not in log or any(
        marker in log for marker in ["panicked at", "VUID-"]
    ):
        raise ValueError("app crashed or GPU validation failed")
    return phases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="New evidence directory")
    parser.add_argument("--trees", type=int, default=8, choices=range(2, 10))
    parser.add_argument("--runs", type=int, default=2)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/re-flora")
    parser.add_argument("--allow-shared-gpu", action="store_true",
                        help="Allow other game instances; timing is explicitly contended")
    parser.add_argument("--app-arg", action="append", default=[],
                        help="Extra application argument, e.g. --app-arg=--no-shadows")
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    out = args.output.resolve()
    if out.exists():
        parser.error("output directory already exists; use a new directory")
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"release binary missing: {binary}")
    env = os.environ.copy()
    env.pop("WAYLAND_DISPLAY", None)
    env["RE_FLORA_TREE_SCALE_DIAGNOSTIC"] = "1"
    command = [str(binary), "--hidden", "--mute", "--perf", "--tree-bench",
               "--tree-bench-samples", str(args.trees), "--auto-exit", "45", *args.app_arg]
    report = {"command": command, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
              "runs": []}
    config_files = [ROOT / "config/gui.toml", ROOT / "config/camera_snapshots.toml"]
    with open("/tmp/re-flora-summer-gpu.lock", "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        active = subprocess.run(["pgrep", "-a", "-x", "re-flora"], capture_output=True,
                                text=True, check=False).stdout
        if active and not args.allow_shared_gpu:
            parser.error("another re-flora instance is running; stop it or use --allow-shared-gpu")
        out.mkdir(parents=True)
        (out / "other-processes.txt").write_text(active)
        (out / "source.diff").write_bytes(subprocess.check_output(["git", "diff"], cwd=ROOT))
        (out / "gpu.txt").write_text(subprocess.run(
            ["nvidia-smi"], capture_output=True, text=True, check=False).stdout)
        original = {path: path.read_bytes() for path in config_files}
        report["contended_at_start"] = bool(active)
        report["gui_sha256"] = hashlib.sha256(original[config_files[0]]).hexdigest()
        try:
            for run in range(1, args.runs + 1):
                log_path = out / f"run-{run}.log"
                with log_path.open("w") as log:
                    result = subprocess.run(command, cwd=ROOT, env=env, stdout=log,
                                            stderr=subprocess.STDOUT, timeout=120, check=False)
                if result.returncode:
                    raise ValueError(f"app exit code {result.returncode}: {log_path}")
                phases = summarize(log_path.read_text(), args.trees)
                report["runs"].append(phases)
                for trees, phase in phases.items():
                    print(f"run={run} trees={trees} median_ms={phase['median_total_ms']:.3f} "
                          f"untracked_cpu_ms={phase['median_untracked_cpu_ms']:.3f} "
                          f"errors={len(phase['errors'])} cliff={phase['frame_cliff']}", flush=True)
                for path, data in original.items():
                    path.write_bytes(data)
        finally:
            for path, data in original.items():
                path.write_bytes(data)
    report["cliff_captured"] = all(any(phase["frame_cliff"] for phase in run.values())
                                   for run in report["runs"])
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"cliff_captured={report['cliff_captured']} contended={report['contended_at_start']}")
    return int(report["cliff_captured"])


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, subprocess.TimeoutExpired) as error:
        print(f"invalid evidence: {error}")
        raise SystemExit(2) from error
