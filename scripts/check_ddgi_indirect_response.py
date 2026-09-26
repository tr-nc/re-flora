#!/usr/bin/env python3
"""Measure raw DDGI receiver response to on/off lighting during 40 edits per transition.

Run from the repository root. Builds Release unless --binary is supplied, serializes GPU
access, and restores saved GUI/camera files. Pass/fail is signal, support, during-edit
response, stable reference, and final decay; latency is reported, not universally budgeted.
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
from itertools import pairwise
from pathlib import Path

PHASES = ("baseline", "on-editing", "on-settling", "off-editing", "off-settling")


def luma(rgb):
    return sum(c * w for c, w in zip(rgb, (0.2126, 0.7152, 0.0722), strict=True))


def analyze(events, static_control=False):
    failures = []
    samples = {phase: [] for phase in PHASES}
    phase_events = [event for event in events if event["event"] == "phase"]
    phases = {event["phase"]: event["ms"] for event in phase_events}
    if [event["phase"] for event in phase_events] != [
        "on-editing",
        "on-settling",
        "off-editing",
        "off-settling",
        "done",
    ]:
        failures.append("missing or unordered response phases/completion")
    previous_serial = 0
    for event in events:
        if event["event"] != "sample":
            continue
        if event["serial"] <= previous_serial:
            failures.append("readback serial did not advance")
        previous_serial = event["serial"]
        if len(event["rgb"]) != 3 or not all(
            math.isfinite(v) and v >= 0 for v in event["rgb"]
        ):
            failures.append("nonfinite or negative irradiance")
            continue
        if not event["ready"]:
            continue  # Startup with no published field is not receiver evidence.
        if (
            event["probes"] <= 0
            or not math.isfinite(event["weight"])
            or event["weight"] <= 0
        ):
            failures.append("receiver has no local DDGI support")
            continue
        samples[event["phase"]].append(event)
    for phase in ("on-editing", "off-editing"):
        edits = [
            event
            for event in events
            if event["event"] == "edit" and event["phase"] == phase
        ]
        if static_control:
            if edits:
                failures.append(f"{phase}: static control unexpectedly edited terrain")
        else:
            if [event["edit"] for event in edits] != list(range(1, 41)):
                failures.append(
                    f"{phase}: expected 40 edits independent of lighting readiness"
                )
            if any(
                b["geometry_revision"] <= a["geometry_revision"]
                for a, b in pairwise(edits)
            ):
                failures.append(
                    f"{phase}: terrain edits did not advance actual geometry"
                )
    if (
        static_control
        and len(
            {
                event["geometry_revision"]
                for values in samples.values()
                for event in values
            }
        )
        != 1
    ):
        failures.append("static control changed geometry")
    references = {}
    for phase in ("baseline", "on-settling", "off-settling"):
        # One value per complete field prevents a long-lived stale field from masquerading
        # as several independent convergence observations.
        reference_samples = samples[phase]
        if static_control and phase != "baseline":
            # Geometry is constant throughout the control, including its timed observation
            # window. Convergence can stop publication before the settling phase starts.
            reference_samples = (
                samples[phase.replace("settling", "editing")] + reference_samples
            )
        fields = {
            event["field_serial"]: luma(event["rgb"]) for event in reference_samples
        }
        values = list(fields.values())[-3:]
        if len(values) < 3:
            failures.append(f"{phase}: fewer than three complete reference fields")
        references[phase] = {
            "values": values,
            "median": statistics.median(values) if values else 0.0,
        }
    dark = references["baseline"]["median"]
    lit = references["on-settling"]["median"]
    final = references["off-settling"]["median"]
    amplitude = lit - dark
    on_ms, off_ms = {}, {}
    if amplitude <= 1e-4:
        failures.append(
            "no measurable indirect-light increase (all-black/frozen output cannot pass)"
        )
    else:
        for phase, reference in references.items():
            values = reference["values"]
            if values and max(values) - min(values) > amplitude * 0.1:
                failures.append(
                    f"{phase}: reference is not stable within 10% of signal"
                )
        if abs(final - dark) > amplitude * 0.05:
            failures.append(
                "removed light leaves more than 5% residual indirect signal"
            )
        for prefix, sign, origin, report in [
            ("on", 1, dark, on_ms),
            ("off", -1, lit, off_ms),
        ]:
            stream = samples[f"{prefix}-editing"] + samples[f"{prefix}-settling"]
            start = phases.get(f"{prefix}-editing", 0)
            for fraction in (0.1, 0.5, 0.9):
                crossing = next(
                    (
                        event["ms"] - start
                        for event in stream
                        if sign * (luma(event["rgb"]) - origin) >= amplitude * fraction
                    ),
                    None,
                )
                report[f"{int(fraction * 100)}_percent"] = crossing
            if not any(
                sign * (luma(event["rgb"]) - origin) >= amplitude * 0.1
                for event in samples[f"{prefix}-editing"]
            ):
                failures.append(
                    f"{prefix}: no meaningful response before editing stopped"
                )
    return {
        "failures": failures,
        "references": references,
        "amplitude": amplitude,
        "on_ms": on_ms,
        "off_ms": off_ms,
        "samples": {phase: len(values) for phase, values in samples.items()},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--spacing", type=int, choices=(16, 32, 64), default=32)
    parser.add_argument(
        "--timeout-seconds",
        type=float,
        default=300,
        help="native auto-exit deadline; incomplete runs fail",
    )
    parser.add_argument(
        "--static-control",
        action="store_true",
        help="same light/receiver timeline without terrain edits",
    )
    parser.add_argument(
        "--binary",
        type=Path,
        help="existing Release executable; skip build, use cwd assets/config",
    )
    args = parser.parse_args()
    if not math.isfinite(args.timeout_seconds) or args.timeout_seconds <= 0:
        parser.error("--timeout-seconds must be finite and positive")
    if args.binary is not None and not (
        args.binary.is_file() and os.access(args.binary, os.X_OK)
    ):
        parser.error("--binary must be executable; omit it to build Release")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if args.binary is None:
        subprocess.run(["cargo", "build", "--release"], check=True)
    binary = (args.binary or Path("target/release/re-flora")).resolve()
    command = [
        str(binary),
        "--hidden",
        "--mute",
        "--windowed",
        "--no-flora",
        "--no-particles",
        "--no-clouds",
        "--no-god-rays",
        "--no-lens-flare",
        "--perf",
        "--environment-lighting-test-scene",
        "indirect-response-static" if args.static_control else "indirect-response",
        "--environment-probe-spacing-voxels",
        str(args.spacing),
        "--auto-exit",
        str(args.timeout_seconds),
    ]
    env = dict(os.environ)
    env.pop("WAYLAND_DISPLAY", None)
    with open("/tmp/re-flora-summer-gpu.lock", "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        paths = [Path("config/gui.toml"), Path("config/camera_snapshots.toml")]
        original = {
            path: path.read_bytes() if path.exists() else None for path in paths
        }
        try:
            gui_hash = hashlib.sha256(paths[0].read_bytes()).hexdigest()
            with (output / "console.log").open("w") as log:
                result = subprocess.run(
                    command, env=env, stdout=log, stderr=subprocess.STDOUT, check=False
                )
            text = (output / "console.log").read_text()
            markers = re.findall(r"\[RUN_LOG\] path=(.*)", text)
            if len(markers) != 1 or not Path(markers[0]).is_file():
                raise RuntimeError(
                    "expected the exact run-log path from this invocation"
                )
            shutil.copyfile(markers[0], output / "run.log")
            (output / "run-log-path.txt").write_text(markers[0] + "\n")
        finally:
            for path, content in original.items():
                if content is None:
                    path.unlink(missing_ok=True)
                else:
                    path.write_bytes(content)
    events = [
        json.loads(line.split("[DDGI_RESPONSE] ", 1)[1])
        for line in text.splitlines()
        if "[DDGI_RESPONSE] " in line
    ]
    report = {
        "command": command,
        "gui_sha256": gui_hash,
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        **analyze(events, static_control=args.static_control),
    }
    report["errors"] = re.findall(
        r".*(?:ERROR|panicked|VUID-|\[CLIMBING\] authored editable).*", text
    )
    report["exit_code"] = result.returncode
    report["passed"] = (
        result.returncode == 0 and not report["failures"] and not report["errors"]
    )
    (output / "samples.json").write_text(json.dumps(events, indent=2) + "\n")
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
