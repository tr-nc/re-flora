#!/usr/bin/env python3
"""Measure sealed-interior edit brightening and a real opening-to-sky control.

Usage: python3 scripts/check_ddgi_cave_edits.py target/cave-edits/baseline
Release GPU test, not a unit test. Restores exact GUI/camera bytes under the GPU lock.
The lower interior-wall ROI excludes sky, HUD, and the outer silhouette. RGB measurements
exclude alpha; linear means are diagnostic, displayed RGB drives the visual tolerance.
"""
import argparse
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess

try:
    from . import ddgi_temporal
except ImportError:
    import ddgi_temporal


def measure(path):
    width, height = map(int, subprocess.check_output(
        ["magick", str(path), "-format", "%w %h", "info:"], text=True).split())
    if abs(width / height - 16 / 9) > 0.01:
        raise ValueError("the fixed receiver ROI requires a 16:9 viewport")
    # x=[25%,75%), y=[60%,80%): interior wall, including in the opened control.
    crop = f"{width // 2}x{height // 5}+{width // 4}+{height * 3 // 5}"
    values = {}
    for space in ("sRGB", "RGB"):
        values[space] = float(subprocess.check_output([
            "magick", str(path), "-alpha", "off",
            "-crop", crop, "-colorspace", space,
            "-format", "%[fx:mean*255]", "info:",
        ], text=True))
    return values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--spacing", type=int, choices=(16, 32, 64), default=32)
    parser.add_argument("--temporal", action="store_true", help="Capture sampled display RGB throughout one real edit run")
    parser.add_argument("--case", choices=("cave-edits", "cave-edits-open", "cave-edits-portal", "terrain-edits-sustained"), default="cave-edits")
    parser.add_argument("--interval", type=float, default=.1)
    parser.add_argument("--duration", type=float, default=58)
    parser.add_argument("--max-mean-jump", type=float, default=3,
                        help="Fail temporal run above this per-ROI mean RGB jump (code values)")
    args = parser.parse_args()
    if not all(math.isfinite(v) and v > 0 for v in (args.interval, args.duration)):
        parser.error("interval and duration must be finite and positive")
    if not math.isfinite(args.max_mean_jump) or args.max_mean_jump < 0:
        parser.error("max-mean-jump must be finite and nonnegative")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error("output directory must be empty; use a fresh path for capture provenance")
    subprocess.run(["cargo", "build", "--release"], check=True)
    report = {"runs": {}, "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
              "git_diff": subprocess.check_output(["git", "diff"], text=True)}
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
            cases = (
                ("active", 0, "cave-edits"),
                ("active-repeat", 0, "cave-edits"),
                ("settled", 55, "cave-edits"),
                ("sealed", 20, "sealed"),
                ("opened", 55, "cave-edits-open"),
            )
            if args.temporal:
                cases = (("temporal", 0, args.case),)
            for name, delay, case in cases:
                command = ["target/release/re-flora", "--hidden", "--mute", "--windowed",
                           "--no-flora", "--no-particles", "--no-clouds", "--no-god-rays",
                           "--no-lens-flare", "--perf", "--environment-lighting-test-scene", case,
                           "--auto-exit", str(args.duration), "--environment-probe-spacing-voxels", str(args.spacing),
                           "--screenshot", "ddgi-cave-repro", str(output / f"{name}.png"),
                           "--screenshot-delay", str(delay)]
                if args.temporal:
                    command += ["--screenshot-sequence", str(int(args.duration / args.interval) + 1), str(args.interval)]
                environment = dict(os.environ)
                environment.pop("WAYLAND_DISPLAY", None)
                with (output / f"{name}.console.log").open("w") as log:
                    result = subprocess.run(command, env=environment, stdout=log, stderr=subprocess.STDOUT)
                text = (output / f"{name}.console.log").read_text()
                log_match = re.search(r"Run log saved to (.+)", text)
                latest = log_match[1].strip() if log_match else None
                if latest:
                    shutil.copyfile(latest, output / f"{name}.run.log")
                begin, end = text.find("[DDGI_SUSTAINED] begin"), text.find("[DDGI_SUSTAINED] end")
                active = text[begin:end] if 0 <= begin < end else ""
                errors = re.findall(r".*(?:ERROR|panicked|VUID-).*", text)
                run = {"command": command, "returncode": result.returncode, "errors": errors,
                       "canonical_log": latest, "edits": active.count("[DDGI_SUSTAINED] edit="),
                       "promotions": len(re.findall(r"staging promoted", active))}
                image = output / f"{name}.png"
                if args.temporal:
                    try:
                        run["temporal"] = ddgi_temporal.analyze(text, image, {
                            "wall_left": (.25, .60, .45, .80),
                            "wall_right": (.55, .60, .75, .80),
                        })
                    except ValueError as error:
                        run["errors"].append(str(error))
                elif image.exists() and f" to {image}" in text:
                    run["roi"] = measure(image)
                report["runs"][name] = run
        finally:
            for path, content in original.items():
                if content is None:
                    path.unlink(missing_ok=True)
                else:
                    path.write_bytes(content)
    runs = report["runs"]
    if args.temporal:
        run = runs["temporal"]
        complete = run["returncode"] == 0 and not run["errors"] and "temporal" in run
        live = run["edits"] == 40 and run["promotions"] >= 2
        coverage = complete and run["temporal"]["capture_gap_seconds"]["max"] <= max(.25, args.interval * 2)
        report["temporal_coverage"] = coverage
        report["passed"] = complete and live and coverage and all(
            roi["temporal"]["mean"]["max"] <= args.max_mean_jump
            for roi in run["temporal"]["rois"].values())
        report["max_mean_jump_rgb_u8"] = args.max_mean_jump
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({"passed": report["passed"], "complete": complete, "live": live,
                          "report": str(output / "report.json")}, indent=2))
        return 0 if report["passed"] else 1
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
