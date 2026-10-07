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

from check_raster_tree_static import setting

ROOT = Path(__file__).resolve().parents[1]
PHASE = re.compile(r"\[TREE_SCALE_DIAG\] phase trees=(\d+)")
SAMPLE = re.compile(
    r"\[TREE_SCALE_DIAG\] sample trees=(\d+) frame=(\d+) "
    r"total_ms=([\d.]+) gpu_present_ms=([\d.]+) untracked_cpu_ms=([\d.]+)"
)
GPU = re.compile(r"([\w.]+)=(\d+)us")
FRAME_SCOPE = re.compile(r"\[PERF\]\[(CPU_FRAME_SCOPE|GPU_FRAME_SCOPE)\] frame (\d+) (.*)")
TREE_UPDATE = re.compile(r"\[PERF\]\[TREE_UPDATE\] frame=(\d+) (.*)")
FRUIT_UPDATE = re.compile(r"\[PERF\]\[FRUIT_UPDATE\] frame=(\d+) (.*)")
CPU_US = re.compile(r"(\w+)_us=([\d.]+)")


def summarize(log, count, validate_gpu=False):
    phases = {i: {"samples": [], "errors": []} for i in range(1, count + 1)}
    current = 0
    updates = {}
    fruit_updates = {}
    frame_scopes = {"CPU_FRAME_SCOPE": {}, "GPU_FRAME_SCOPE": {}}
    for line in log.splitlines():
        update = TREE_UPDATE.search(line)
        if update:
            updates[int(update[1])] = {name: float(value) for name, value in CPU_US.findall(update[2])}
        fruit_update = FRUIT_UPDATE.search(line)
        if fruit_update:
            fruit_updates[int(fruit_update[1])] = {
                name: float(value) for name, value in CPU_US.findall(fruit_update[2])
            }
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
        scope = FRAME_SCOPE.search(line)
        if scope:
            frame_scopes[scope[1]][int(scope[2])] = {
                name: int(duration) for name, duration in GPU.findall(scope[3])
            }
    for tree_count, phase in phases.items():
        if len(phase["samples"]) < 3:
            raise ValueError(f"tree count {tree_count}: fewer than three settled frames")
        for metric in ["total_ms", "gpu_present_ms", "untracked_cpu_ms"]:
            values = [sample[metric] for sample in phase["samples"]]
            phase[f"median_{metric}"] = statistics.median(values)
            phase[f"max_{metric}"] = max(values)
        cpu = [updates[sample["frame"]] for sample in phase["samples"] if sample["frame"] in updates]
        phase["cpu_update_medians_us"] = {
            name: statistics.median(row[name] for row in cpu if name in row)
            for name in sorted({key for row in cpu for key in row})
        }
        fruit = [fruit_updates[sample["frame"]] for sample in phase["samples"]
                 if sample["frame"] in fruit_updates]
        phase["fruit_update_medians_us"] = {
            name: statistics.median(row[name] for row in fruit if name in row)
            for name in sorted({key for row in fruit for key in row})
        }
        for kind, label in [("CPU_FRAME_SCOPE", "cpu"), ("GPU_FRAME_SCOPE", "gpu")]:
            rows = [frame_scopes[kind][sample["frame"]] for sample in phase["samples"]
                    if sample["frame"] in frame_scopes[kind]]
            scopes = {
                name: [row[name] for row in rows if name in row]
                for name in sorted({key for row in rows for key in row})
            }
            phase[f"{label}_scopes_us"] = scopes
            phase[f"{label}_scope_medians_us"] = {
                name: statistics.median(values) for name, values in scopes.items()
            }
            phase[f"{label}_scope_sample_counts"] = {name: len(values) for name, values in scopes.items()}
        prior = phases.get(tree_count - 1)
        phase["frame_cliff"] = bool(
            prior and phase["median_total_ms"] >= 80
            and phase["median_total_ms"] >= 3 * prior["median_total_ms"]
        )
    if validate_gpu and any(
        f"[TREE_SCALE_DIAG] validated trees={i}" not in log for i in phases
    ):
        raise ValueError("missing GPU surface/collision validation")
    if f"[TREE_SCALE_DIAG] completed trees={count}" not in log:
        raise ValueError("tree replay did not complete")
    if "Application exited successfully" not in log or any(
        marker in log for marker in ["panicked at", "VUID-"]
    ):
        raise ValueError("app crashed or GPU validation failed")
    return phases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="New evidence directory")
    parser.add_argument("--trees", type=int, default=8, choices=range(2, 13))
    parser.add_argument("--runs", type=int, default=2)
    parser.add_argument("--final-seconds", type=float, default=2,
                        help="Hold the final forest for this many seconds (at least 2); earlier phases stay 2s")
    parser.add_argument("--species", choices=["garden", "conifer"], default="garden",
                        help="Species added through the same description compiler as Grow")
    parser.add_argument("--seed", type=int, default=123, help="First added tree seed")
    parser.add_argument("--tree-size", type=float, help="Diagnostic input size, not a new default")
    parser.add_argument("--wind-off", action="store_true", help="One-variable wind ablation")
    parser.add_argument("--fruit-cycle", type=float,
                        help="Temporary fruit season [0,1]; 0 prevents dropped-body work, restores saved config on exit")
    parser.add_argument("--validate-gpu", action="store_true",
                        help="Validate CPU/GPU surface, BVH, lighting and collision between phases")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/re-flora")
    parser.add_argument("--allow-shared-gpu", action="store_true",
                        help="Allow other game instances; timing is explicitly contended")
    parser.add_argument("--app-arg", action="append", default=[],
                        help="Extra application argument, e.g. --app-arg=--no-shadows")
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    if not 2 <= args.final_seconds < float("inf"):
        parser.error("--final-seconds must be finite and at least 2")
    if not 0 <= args.seed <= 2**64 - args.trees + 1:
        parser.error("--seed must fit u64 including subsequent placement seeds")
    if args.fruit_cycle is not None and not 0 <= args.fruit_cycle <= 1:
        parser.error("--fruit-cycle must be finite and in [0,1]")
    if args.tree_size is not None and not 0 < args.tree_size < float("inf"):
        parser.error("--tree-size must be positive and finite")
    out = args.output.resolve()
    if out.exists():
        parser.error("output directory already exists; use a new directory")
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"release binary missing: {binary}")
    env = os.environ.copy()
    env.pop("WAYLAND_DISPLAY", None)
    env["RE_FLORA_TREE_SCALE_DIAGNOSTIC"] = "1"
    env["RE_FLORA_TREE_SCALE_SPECIES"] = args.species
    env["RE_FLORA_TREE_SCALE_SEED"] = str(args.seed)
    env["RE_FLORA_TREE_SCALE_FINAL_SECONDS"] = str(args.final_seconds)
    if args.validate_gpu:
        env["RE_FLORA_TREE_SCALE_VALIDATE_GPU"] = "1"
    else:
        env.pop("RE_FLORA_TREE_SCALE_VALIDATE_GPU", None)
    auto_exit = max(45, (args.trees - 1) * 2 + args.final_seconds + 20)
    command = [str(binary), "--hidden", "--mute", "--perf", "--tree-bench",
               "--tree-bench-samples", str(args.trees), "--auto-exit", str(auto_exit), *args.app_arg]
    report = {"command": command, "species": args.species, "first_added_seed": args.seed,
              "tree_size_override": args.tree_size, "wind_off": args.wind_off,
              "fruit_cycle_override": args.fruit_cycle,
              "validate_gpu": args.validate_gpu, "final_seconds": args.final_seconds,
              "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
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
        source = original[config_files[0]].decode()
        if args.tree_size is not None:
            if args.tree_size <= 0:
                raise ValueError("--tree-size must be positive")
            source, matches = re.subn(r"(\[tree\.desc\]\nsize = )[^\n]+",
                                      lambda match: match[1] + str(args.tree_size), source)
            if matches != 1:
                raise ValueError("missing authored tree size")
        if args.wind_off:
            source = setting(source, "raster_tree_wind", "false")
        if args.fruit_cycle is not None:
            source = setting(source, "fruit_cycle", str(args.fruit_cycle))
        try:
            for run in range(1, args.runs + 1):
                config_files[0].write_text(source)
                log_path = out / f"run-{run}.log"
                with log_path.open("w") as log:
                    result = subprocess.run(command, cwd=ROOT, env=env, stdout=log,
                                            stderr=subprocess.STDOUT, timeout=max(120, auto_exit + 30), check=False)
                if result.returncode:
                    raise ValueError(f"app exit code {result.returncode}: {log_path}")
                phases = summarize(log_path.read_text(), args.trees, args.validate_gpu)
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
