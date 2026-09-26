#!/usr/bin/env python3
"""Release/GPU regression: complete lighting must advance before a held edit ends.

Usage: python3 scripts/check_ddgi_sustained_edits.py target/edit-lighting/baseline
Builds release (or uses --binary), serializes GPU access, restores GUI/camera bytes even on failure.
Checks progressive publication and final-revision catch-up; timing metrics are diagnostic,
not a universal latency/performance threshold. Run from the repository root.
Requires ImageMagick (`magick`) for a diagnostic receiver measurement, not a brightness floor.
Terrain now displays physical estimates directly; the old --fallback-strength option is retired.
"""
import argparse
import fcntl
import hashlib
import json
import math
import os
import re
import shutil
import statistics
import subprocess
import sys
from itertools import pairwise
from pathlib import Path


def timing_summary(values):
    ordered = sorted(values)
    return {"samples": len(ordered),
            "median": statistics.median(ordered) if ordered else None,
            "p95": ordered[math.ceil(len(ordered) * 0.95) - 1] if ordered else None,
            "max": max(ordered) if ordered else None}


def analyze_log(text):
    """Check publication liveness, not pixel brightness or physical convergence."""
    events = []
    day = 0
    previous = 0
    for line in text.splitlines():
        match = re.match(r"\[(\d\d):(\d\d):(\d\d)\.(\d{3}) ", line)
        if match is None:
            continue
        h, m, s, ms = map(int, match.groups())
        clock = ((h * 60 + m) * 60 + s) * 1000 + ms
        if clock < previous - 12 * 60 * 60 * 1000:
            day += 24 * 60 * 60 * 1000
        previous = clock
        events.append((day + clock, line))
    starts = [ms for ms, line in events if "[DDGI_SUSTAINED] begin" in line]
    ends = [ms for ms, line in events if "[DDGI_SUSTAINED] end" in line]
    bounded = len(starts) == len(ends) == 1 and ends[0] > starts[0]
    begin, end = (starts[0], ends[0]) if bounded else (0, 0)
    active = [(ms, line) for ms, line in events if bounded and begin <= ms <= end]
    edits = [(ms, int(match[1]), int(match[2])) for ms, line in active
             if (match := re.search(r"\[DDGI_SUSTAINED\] edit=(\d+) revision=(\d+)", line))]
    publications = [(ms, int(match[1])) for ms, line in events
                    if (match := re.search(r"staging promoted .*?geometry_revision=(\d+)", line))]
    during = [(ms, rev) for ms, rev in publications if bounded and begin <= ms < end]
    # Initial in-flight geometry may publish after 'begin' but before any edit is covered.
    useful = [(ms, rev) for ms, rev in during if edits and rev >= edits[0][2]]
    final_revision = edits[-1][2] if edits else None
    final_time = next((ms for ms, rev in publications
                       if bounded and ms >= end and rev == final_revision), None)
    revisions = [rev for _, rev in during]
    gaps = [b[0] - a[0] for a, b in pairwise(useful)]
    boundaries = [begin] + [ms for ms, _ in useful] + [end]
    frame_us = [int(match[1]) for _, line in active
                if (match := re.search(r"frame\.render=(\d+)us", line))]
    errors = re.findall(r".*(?:ERROR|panicked|VUID-).*", text)
    failures = []
    if not bounded:
        failures.append("expected one complete sustained-edit begin/end interval")
    if [number for _, number, _ in edits] != list(range(1, 41)):
        failures.append("expected all 40 ordered edits")
    if any(b[2] <= a[2] for a, b in pairwise(edits)):
        failures.append("edit terrain revisions did not strictly advance")
    if len({rev for _, rev in useful}) < 2:
        failures.append("fewer than two distinct edited fields published during editing")
    if any(b <= a for a, b in pairwise(revisions)):
        failures.append("during-edit publication revisions did not strictly advance")
    if final_time is None:
        failures.append("final terrain revision did not publish after editing ended")
    if errors:
        failures.append("runtime errors in log")
    return {"validation_failures": failures,
            "screenshot_during_edits": any("[SCREENSHOT] Saved" in line for _, line in active),
            "edits": len(edits), "edit_duration_ms": end - begin if bounded else None,
            "promoted_during_edits": [str(rev) for rev in revisions],
            "useful_promotions_during_edits": [rev for _, rev in useful],
            "first_useful_publication_ms": useful[0][0] - begin if useful else None,
            "publication_gap_ms": timing_summary(gaps),
            "max_no_publication_ms": max((b - a for a, b in pairwise(boundaries)),
                                         default=0) if bounded else None,
            "final_revision": final_revision,
            "final_catchup_ms": final_time - end if final_time is not None else None,
            "errors": errors,
            "render_us_mean": statistics.mean(frame_us) if frame_us else None,
            "render_us_max": max(frame_us) if frame_us else None}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--spacing", type=int, choices=(16, 32, 64), default=32)
    parser.add_argument("--binary", type=Path,
                        help="use an existing Release executable without rebuilding; assets/config come from cwd")
    args = parser.parse_args()
    if args.binary is not None and not (args.binary.is_file() and os.access(args.binary, os.X_OK)):
        parser.error("--binary must name an executable file; omit it to build with cargo build --release")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if args.binary is None:
        subprocess.run(["cargo", "build", "--release"], check=True)
    binary = (args.binary or Path("target/release/re-flora")).resolve()
    command = [str(binary), "--hidden", "--mute", "--windowed",
               "--no-flora", "--no-particles", "--no-clouds", "--no-god-rays",
               "--no-lens-flare", "--perf", "--environment-lighting-test-scene",
               "terrain-edits-sustained", "--auto-exit", "18",
               "--environment-probe-spacing-voxels", str(args.spacing),
               "--screenshot", "ddgi-edit-repro", str(output / "editing.png"),
               "--screenshot-delay", "0"]
    environment = dict(os.environ)
    environment.pop("WAYLAND_DISPLAY", None)
    with open("/tmp/re-flora-summer-gpu.lock", "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        paths = [Path("config/gui.toml"), Path("config/camera_snapshots.toml")]
        original = {p: p.read_bytes() if p.exists() else None for p in paths}
        try:
            effective_gui_sha256 = hashlib.sha256(paths[0].read_bytes()).hexdigest()
            paths[1].write_text("""[[snapshots]]
name = "ddgi-edit-repro"
position = [0.65, 0.52, 1.38]
yaw_deg = 0.0
pitch_deg = 42.878903
fov_deg = 60.0
fly_mode = true
""")
            with (output / "console.log").open("w") as log:
                result = subprocess.run(command, env=environment, stdout=log,
                                        stderr=subprocess.STDOUT, check=False)
            latest = subprocess.check_output([str(binary), "--latest-log"], text=True).strip()
            (output / "run-log-path.txt").write_text(latest + "\n")
            shutil.copyfile(latest, output / "run.log")
        finally:
            for path, content in original.items():
                if content is None:
                    path.unlink(missing_ok=True)
                else:
                    path.write_bytes(content)
    text = (output / "console.log").read_text()
    report = {"command": command,
              "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "gui_sha256": effective_gui_sha256,
              **analyze_log(text)}
    # Measure the newly exposed left skylight reveal without imposing a nonzero
    # floor: a physical estimate may be dark or unconverged. Display wiring is
    # covered by terrain_and_raster_consumers_share_the_ddgi_sampler_contract;
    # this real GPU run verifies publication liveness and records visual evidence.
    pixel = None
    if (output / "editing.png").exists():
        sample = subprocess.check_output([
            "magick", str(output / "editing.png"), "-format",
            "%w %h %[fx:255*(p{w*0.39453125,h*0.20833333}.r+p{w*0.39453125,h*0.20833333}.g+p{w*0.39453125,h*0.20833333}.b)/3]",
            "info:"], text=True).split()
        width, height, pixel = int(sample[0]), int(sample[1]), float(sample[2])
        report["image_dimensions"] = [width, height]
        if abs(width / height - 16 / 9) > 0.01:
            raise ValueError("fixed receiver measurement requires the fixture's 16:9 viewport")
    report["latest_estimate_receiver_rgb_mean_u8"] = pixel
    passed = (result.returncode == 0 and not report["validation_failures"]
              and report["screenshot_during_edits"] and pixel is not None)
    report["exit_code"] = result.returncode
    report["passed"] = passed
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
