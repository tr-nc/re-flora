#!/usr/bin/env python3
"""Check native terrain edits do not rebuild unrelated raster trees.

Run from the worktree being tested, after cargo build --release. Uses the normal
world's real three-step water-edit-soak brush replay, without lighting fixtures,
screenshots, or disabling vegetation. Temporarily enables raster trees and restores
GUI/camera bytes. Exit 1 means the workload or no-unrelated-rebuild contract failed.
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


def summarize(values):
    if not values:
        return None
    ordered = sorted(values)
    return {
        "samples": len(values),
        "median": statistics.median(values),
        "p95": ordered[int((len(ordered) - 1) * 0.95)],
        "max": ordered[-1],
    }


def timestamp(line):
    match = re.match(r"\[(\d+):(\d+):(\d+\.\d+)", line)
    if match:
        return int(match[1]) * 3600 + int(match[2]) * 60 + float(match[3])
    return None


def analyze(text):
    lines = text.splitlines()
    edits = [timestamp(line) for line in lines if "[WATER][EDIT_SOAK] applied" in line]
    edits = [t for t in edits if t is not None]
    errors = re.findall(r".*(?:ERROR|panicked|VUID-).*", text)
    active = []
    if len(edits) == 3:
        span = (edits[-1] - edits[0]) % 86400 + 1
        active = [
            line
            for line in lines
            if (t := timestamp(line)) is not None and (t - edits[0]) % 86400 <= span
        ]
    tree_marker = "[TREE][RASTER_STATIC] revision="
    compiles = [line for line in lines if tree_marker in line]
    rebuilds = [line for line in active if tree_marker in line]
    frame_ms = [
        float(m[1])
        for line in active
        if (m := re.search(r"\[PERF\]\[FRAME\].*?total ([\d.]+)ms", line))
    ]
    return {
        "edits": len(edits),
        "errors": errors,
        "tree_compiles": len(compiles),
        "during_edit_rebuilds": len(rebuilds),
        "logged_edit_frame_ms": summarize(frame_ms),
        "passed": len(edits) == 3
        and len(compiles) == 1
        and not rebuilds
        and bool(frame_ms)
        and not errors
        and "completed deterministic terrain-edit sequence" in text
        and "Application exited successfully" in text,
    }


def terminate(signum, _frame):
    raise SystemExit(128 + signum)


def main():
    parser = argparse.ArgumentParser(
        description=__doc__,
        epilog=(
            "Example: cargo build --release && python3 scripts/check_tree_terrain_edit_perf.py "
            "target/tree-edit-perf-01"
        ),
    )
    parser.add_argument(
        "output", type=Path, help="fresh evidence directory (must be empty)"
    )
    args = parser.parse_args()
    root = Path.cwd()
    binary = root / "target/release/re-flora"
    if not binary.is_file() or not (root / "config/gui.toml").is_file():
        parser.error("run from the tested worktree after cargo build --release")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error("output directory is not empty; choose a fresh evidence directory")
    command = [
        str(binary),
        "--hidden",
        "--mute",
        "--perf",
        "--water-edit-soak",
        "--auto-exit",
        "10",
    ]
    report = {
        "command": command,
        "head": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], text=True
        ).strip(),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    }
    (output / "source.diff").write_bytes(
        subprocess.check_output(["git", "diff", "HEAD"])
    )
    signal.signal(signal.SIGTERM, terminate)
    env = dict(os.environ)
    env.pop("WAYLAND_DISPLAY", None)
    with open("/tmp/re-flora-summer-gpu.lock", "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        paths = [
            root / "config" / name for name in ("gui.toml", "camera_snapshots.toml")
        ]
        saved = {p: p.read_bytes() if p.exists() else None for p in paths}
        for p, data in saved.items():
            if data is not None:
                (output / (p.name + ".before")).write_bytes(data)
        try:
            gui, count = re.subn(
                r'(id = "raster_tree_static"\n(?:(?!\[\[section).)*?value = )(true|false)',
                lambda m: m[1] + "true",
                paths[0].read_text(),
                flags=re.DOTALL,
            )
            if count != 1:
                raise ValueError("expected exactly one raster_tree_static control")
            paths[0].write_text(gui)
            report["gui_sha256"] = hashlib.sha256(paths[0].read_bytes()).hexdigest()
            with (output / "run.log").open("w") as log:
                result = subprocess.run(
                    command,
                    env=env,
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    timeout=90,
                    check=False,
                )
            report.update(analyze((output / "run.log").read_text()))
            report["returncode"] = result.returncode
            report["passed"] &= result.returncode == 0
        finally:
            for p, data in saved.items():
                if data is None:
                    p.unlink(missing_ok=True)
                else:
                    p.write_bytes(data)
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
