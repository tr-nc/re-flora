#!/usr/bin/env python3
"""DIAGNOSTIC ONLY: real Release app near/far tree-edit replay; no performance fix.

Build with cargo build --release first. Exit 1 means the near-only >=100ms stall
was captured in every pair; 0 means not captured; 2 means invalid evidence.
Uses RE_FLORA_TREE_EDIT_DIAGNOSTIC probes, one removal at render time 5s and
exit at 7s. Keeps native render settings and restores GUI/camera bytes.
"""

import argparse
import fcntl
import hashlib
import json
import os
import re
import signal
import statistics
import subprocess
from pathlib import Path


def analyze(text):
    lines = text.splitlines()
    starts = [i for i, line in enumerate(lines) if "[TREE_EDIT_DIAG] begin" in line]
    if len(starts) != 1:
        raise ValueError(f"expected one edit, got {len(starts)}")
    before, after = lines[: starts[0]], lines[starts[0] :]

    def frames(rows):
        return [float(m[1]) for line in rows if (m := re.search(
            r"\[PERF\]\[FRAME\].*?total ([\d.]+)ms", line))]

    active = frames(after)
    if not active or not frames(before):
        raise ValueError("missing frame timings")
    if "Application exited successfully" not in text or re.search(
        r"ERROR|panicked|VUID-", text
    ):
        raise ValueError("app did not finish cleanly")
    edit_ms = re.search(r"\[TREE_EDIT_DIAG\] end step=0 edit_ms=([\d.]+)", text)
    if edit_ms is None:
        raise ValueError("edit did not complete")
    return {
        "warm_last_120_frame_median_ms": statistics.median(frames(before)[-120:]),
        "edit_ms": float(edit_ms[1]),
        "edit_frame_ms": active[0],
        "post_edit_max_frame_ms": max(active),
        "post_edit_frame_count": len(active),
        "post_edit_tree_compiles": sum("[TREE][RASTER_STATIC] revision=" in s for s in after),
        "evidence": [s for s in after if any(marker in s for marker in (
            "[TREE_EDIT_DIAG]", "[WATER][EDIT_SOAK] applied",
            "[PERF][VISIBLE_TERRAIN_PUBLICATION]", "[TREE][RASTER_STATIC] revision=",
            "[TREE][NORMAL_CONFIDENCE]",
        ))],
    }


def terminate(signum, _frame):
    raise SystemExit(128 + signum)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="fresh evidence directory")
    parser.add_argument("--repeats", type=int, default=3, help="near/far pairs (default 3)")
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("--repeats must be positive")
    root = Path.cwd()
    binary = root / "target/release/re-flora"
    if not binary.is_file():
        parser.error("run cargo build --release in the tested worktree first")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error("evidence directory must be empty")
    signal.signal(signal.SIGTERM, terminate)
    command = [str(binary), "--hidden", "--mute", "--perf", "--water-edit-soak", "--auto-exit", "7"]
    report = {
        "diagnostic_only": True,
        "head": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "command": command,
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "runs": [],
    }
    (output / "source.diff").write_bytes(subprocess.check_output(["git", "diff", "HEAD"]))
    paths = [root / "config" / name for name in ("gui.toml", "camera_snapshots.toml")]
    with open("/tmp/re-flora-summer-gpu.lock", "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        saved = {p: p.read_bytes() if p.exists() else None for p in paths}
        for path, data in saved.items():
            if data is not None:
                (output / (path.name + ".before")).write_bytes(data)
        try:
            for repeat in range(args.repeats):
                modes = ("near", "far") if repeat % 2 == 0 else ("far", "near")
                for mode in modes:
                    # Refuse known competing app instances; desktop compositor noise remains.
                    processes = subprocess.check_output(["ps", "-eo", "pid,comm,args"], text=True)
                    if any(len(parts := row.split()) > 1 and parts[1] == "re-flora"
                           for row in processes.splitlines()):
                        raise RuntimeError("another re-flora instance is active; retry after it exits")
                    label = f"{mode}-{repeat + 1}"
                    (output / f"{label}.processes.txt").write_text(processes)
                    with (output / f"{label}.gpu.txt").open("w") as gpu:
                        subprocess.run(["nvidia-smi"], stdout=gpu, stderr=subprocess.STDOUT, check=False)
                    env = dict(os.environ, RE_FLORA_TREE_EDIT_DIAGNOSTIC=mode)
                    env.pop("WAYLAND_DISPLAY", None)
                    with (output / f"{label}.log").open("w") as log:
                        result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT,
                                                timeout=90, check=False)
                    if result.returncode:
                        raise RuntimeError(f"{label}: app exit {result.returncode}")
                    entry = analyze((output / f"{label}.log").read_text())
                    entry.update(mode=mode, repeat=repeat + 1)
                    report["runs"].append(entry)
                    for path, data in saved.items():
                        if data is not None:
                            path.write_bytes(data)
                        else:
                            path.unlink(missing_ok=True)
            near = [r for r in report["runs"] if r["mode"] == "near"]
            far = [r for r in report["runs"] if r["mode"] == "far"]
            report["stall_captured"] = all(r["edit_frame_ms"] >= 100 for r in near) and all(
                r["edit_frame_ms"] < 100 for r in far)
            code = 1 if report["stall_captured"] else 0
        except (RuntimeError, ValueError, subprocess.TimeoutExpired) as error:
            report["error"] = str(error)
            code = 2
        finally:
            for path, data in saved.items():
                if data is not None:
                    path.write_bytes(data)
                else:
                    path.unlink(missing_ok=True)
            (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k != "runs"}, indent=2))
    for run in report["runs"]:
        print(run["mode"], run["repeat"], "edit frame ms", run["edit_frame_ms"],
              "tree compiles", run["post_edit_tree_compiles"])
    return code


if __name__ == "__main__":
    raise SystemExit(main())
