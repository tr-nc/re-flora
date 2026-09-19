#!/usr/bin/env python3
"""Measure sealed-interior edit brightening and a real opening-to-sky control.

Usage: python3 scripts/check_ddgi_cave_edits.py target/cave-edits/baseline
Release GPU test, not a unit test. Restores exact GUI/camera bytes under the GPU lock.
The central image ROI excludes sky, HUD, and the outer silhouette. RGB measurements
exclude alpha; linear means are diagnostic, displayed RGB drives the visual tolerance.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


def measure(path):
    values = {}
    for space in ("sRGB", "RGB"):
        values[space] = float(subprocess.check_output([
            "magick", str(path), "-alpha", "off", "-gravity", "center",
            "-crop", "50%x50%+0+0", "-colorspace", space,
            "-format", "%[fx:mean*255]", "info:",
        ], text=True))
    return values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--spacing", type=int, choices=(16, 32, 64), default=32)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    subprocess.run(["cargo", "build", "--release"], check=True)
    report = {"runs": {}}
    with open("/tmp/re-flora-summer-gpu.lock", "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        paths = [Path("config/gui.toml"), Path("config/camera_snapshots.toml")]
        original = {p: p.read_bytes() if p.exists() else None for p in paths}
        try:
            report["gui_sha256"] = hashlib.sha256(paths[0].read_bytes()).hexdigest()
            paths[1].write_text('''[[snapshots]]
name = "ddgi-cave-repro"
position = [0.65, 0.52, 1.38]
yaw_deg = 0.0
pitch_deg = 42.878903
fov_deg = 60.0
fly_mode = true
''')
            for name, delay, case in (
                ("active", 0, "cave-edits"),
                ("active-repeat", 0, "cave-edits"),
                ("settled", 55, "cave-edits"),
                ("sealed", 20, "sealed"),
                ("opened", 55, "cave-edits-open"),
            ):
                command = ["target/release/re-flora", "--hidden", "--mute", "--windowed",
                           "--no-flora", "--no-particles", "--no-clouds", "--no-god-rays",
                           "--no-lens-flare", "--perf", "--environment-lighting-test-scene", case,
                           "--auto-exit", "58", "--environment-probe-spacing-voxels", str(args.spacing),
                           "--screenshot", "ddgi-cave-repro", str(output / f"{name}.png"),
                           "--screenshot-delay", str(delay)]
                environment = dict(os.environ)
                environment.pop("WAYLAND_DISPLAY", None)
                with (output / f"{name}.console.log").open("w") as log:
                    result = subprocess.run(command, env=environment, stdout=log, stderr=subprocess.STDOUT)
                latest = subprocess.check_output(["target/release/re-flora", "--latest-log"], text=True).strip()
                shutil.copyfile(latest, output / f"{name}.run.log")
                text = (output / f"{name}.console.log").read_text()
                begin, end = text.find("[DDGI_SUSTAINED] begin"), text.find("[DDGI_SUSTAINED] end")
                active = text[begin:end] if 0 <= begin < end else ""
                errors = re.findall(r".*(?:ERROR|panicked|VUID-).*", text)
                run = {"command": command, "returncode": result.returncode, "errors": errors,
                       "canonical_log": latest, "edits": active.count("[DDGI_SUSTAINED] edit="),
                       "promotions": len(re.findall(r"staging promoted", active))}
                image = output / f"{name}.png"
                if image.exists():
                    run["roi"] = measure(image)
                report["runs"][name] = run
        finally:
            for path, content in original.items():
                if content is None:
                    path.unlink(missing_ok=True)
                else:
                    path.write_bytes(content)
    runs = report["runs"]
    complete = all(r["returncode"] == 0 and not r["errors"] and "roi" in r for r in runs.values())
    live = all(runs[n]["edits"] == 40 and runs[n]["promotions"] >= 2
               for n in ("active", "active-repeat", "settled", "opened"))
    if complete:
        settled = runs["settled"]["roi"]["sRGB"]
        report["excess_display_rgb_u8"] = {n: runs[n]["roi"]["sRGB"] - settled
                                           for n in ("active", "active-repeat")}
        # A three-code-value mean excess is visible well above near-black quantization.
        report["no_edit_brightening"] = max(report["excess_display_rgb_u8"].values()) <= 3
        report["opening_brightens"] = runs["opened"]["roi"]["sRGB"] > settled + 10
    report["passed"] = complete and live and report["no_edit_brightening"] and report["opening_brightens"]
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
