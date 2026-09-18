#!/usr/bin/env python3
"""Release/GPU regression: complete lighting must advance before a held edit ends.

Usage: python3 scripts/check_ddgi_sustained_edits.py target/edit-lighting/baseline
Builds release, serializes GPU access, restores GUI/camera bytes even on failure.
"""
import argparse
import hashlib
import shutil
import fcntl
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--spacing", type=int, choices=(16, 32, 64), default=32)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cargo", "build", "--release"], check=True)
    command = ["target/release/re-flora", "--hidden", "--mute", "--windowed",
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
            paths[1].write_text("""[[snapshots]]
name = "ddgi-edit-repro"
position = [0.65, 0.52, 1.38]
yaw_deg = 0.0
pitch_deg = 42.878903
fov_deg = 60.0
fly_mode = true
""")
            with (output / "console.log").open("w") as log:
                result = subprocess.run(command, env=environment, stdout=log, stderr=subprocess.STDOUT)
            latest = subprocess.check_output(["target/release/re-flora", "--latest-log"], text=True).strip()
            (output / "run-log-path.txt").write_text(latest + "\n")
            shutil.copyfile(latest, output / "run.log")
        finally:
            for path, content in original.items():
                if content is None:
                    path.unlink(missing_ok=True)
                else:
                    path.write_bytes(content)
    text = (output / "console.log").read_text()
    begin = text.find("[DDGI_SUSTAINED] begin")
    end = text.find("[DDGI_SUSTAINED] end")
    active = text[begin:end] if begin >= 0 and end > begin else ""
    promotions = re.findall(r"staging promoted .*?geometry_revision=(\d+)", active)
    frame_us = [int(v) for v in re.findall(r"frame\.render=(\d+)us", active)]
    errors = re.findall(r".*(?:ERROR|panicked|VUID-).*", text)
    report = {"command": command,
              "gui_sha256": hashlib.sha256(original[paths[0]]).hexdigest(),
              "screenshot_during_edits": "[SCREENSHOT] Saved" in active, "edits": active.count("[DDGI_SUSTAINED] edit="),
              "promoted_during_edits": promotions, "errors": errors,
              "render_us_mean": sum(frame_us) / len(frame_us) if frame_us else None,
              "render_us_max": max(frame_us) if frame_us else None}
    passed = result.returncode == 0 and end > begin >= 0 and len(promotions) >= 2 and "[SCREENSHOT] Saved" in active and not errors
    report["passed"] = passed
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
