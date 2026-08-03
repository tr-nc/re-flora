#!/usr/bin/env python3
"""Measure release-mode DDGI staging publication and matched frame scopes.

The runner intentionally uses the terrain-edits-closed acceptance scene by default because it
performs a real active/staging DDGI Volume publication. It does not change synchronization; it
records the current publication seam so Ticket 08 can compare a replacement against the same
workload and scene ancestry.
"""

from __future__ import annotations

import argparse
import json
import platform
import re
import statistics
import subprocess
import sys
import socket
from datetime import datetime, timezone
from pathlib import Path

PUBLICATION_RE = re.compile(
    r"\[DDGI\]\[PUBLICATION_TIMING\] token_serial=(?P<token>\d+) "
    r"descriptor_rebind_ms=(?P<descriptor>[0-9.]+) "
    r"resource_swap_ms=(?P<swap>[0-9.]+) "
    r"total_publication_ms=(?P<total>[0-9.]+) "
    r"descriptor_generation=(?P<generation>\d+)"
)
FRAME_RE = re.compile(
    r"\[PERF\]\[GPU_FRAME_SCOPE\] frame (?P<frame>\d+) .*?frame\.render=(?P<render>\d+)us"
)
DEVICE_RE = re.compile(r"Selected physical device: (?P<device>.+)")


def summary(values: list[float]) -> dict[str, float | int]:
    if not values:
        raise ValueError("cannot summarize an empty sample set")
    ordered = sorted(values)
    p95_index = min(len(ordered) - 1, int((len(ordered) - 1) * 0.95))
    return {
        "samples": len(values),
        "mean": statistics.fmean(values),
        "median": statistics.median(values),
        "p95": ordered[p95_index],
        "min": ordered[0],
        "max": ordered[-1],
    }


def command(args: argparse.Namespace, capture_path: Path) -> list[str]:
    return [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "--",
        "--hidden",
        "--mute",
        "--no-flora",
        "--no-particles",
        "--no-god-rays",
        "--no-lens-flare",
        "--no-clouds",
        "--perf",
        "--environment-lighting-test-scene",
        args.scene,
        "--environment-probe-spacing-voxels",
        str(args.spacing_voxels),
        "--auto-exit",
        str(args.auto_exit),
        "--environment-irradiance-capture",
        str(capture_path),
        "--environment-irradiance-capture-target",
        "published",
    ]


def parse_log(text: str) -> dict[str, object]:
    publications = [
        {
            "token_serial": int(match.group("token")),
            "descriptor_rebind_ms": float(match.group("descriptor")),
            "resource_swap_ms": float(match.group("swap")),
            "total_publication_ms": float(match.group("total")),
            "descriptor_generation": int(match.group("generation")),
        }
        for match in PUBLICATION_RE.finditer(text)
    ]
    frame_render_us = [int(match.group("render")) for match in FRAME_RE.finditer(text)]
    device_match = DEVICE_RE.search(text)
    if not publications:
        raise ValueError(
            "no DDGI publication timing marker found; increase --auto-exit or verify the "
            "radiance-changes scene reached a staging promotion"
        )
    return {
        "device": device_match.group("device").strip() if device_match else None,
        "publications": publications,
        "frame_render_us": frame_render_us,
    }


def run(args: argparse.Namespace) -> int:
    repo = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    samples: list[dict[str, object]] = []
    commands: list[list[str]] = []

    for index in range(1, args.samples + 1):
        capture = output / f"sample-{index:02d}.rfirr"
        log_path = output / f"sample-{index:02d}.console.log"
        cmd = command(args, capture)
        commands.append(cmd)
        if args.dry_run:
            print(" ".join(cmd))
            continue
        completed = subprocess.run(cmd, cwd=repo, text=True, capture_output=True, check=False)
        text = completed.stdout + completed.stderr
        log_path.write_text(text, encoding="utf-8")
        if completed.returncode != 0:
            print(f"sample {index} failed with exit status {completed.returncode}; see {log_path}", file=sys.stderr)
            return completed.returncode or 1
        try:
            parsed = parse_log(text)
        except ValueError as error:
            print(f"sample {index} failed: {error}; see {log_path}", file=sys.stderr)
            return 1
        parsed["sample"] = index
        parsed["log"] = str(log_path)
        samples.append(parsed)

    if args.dry_run:
        return 0

    publications = [publication for sample in samples for publication in sample["publications"]]
    frame_render_us = [value for sample in samples for value in sample["frame_render_us"]]
    summary_payload = {
        "version": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "host": socket.gethostname(),
        "platform": platform.platform(),
        "scene": args.scene,
        "spacing_voxels": args.spacing_voxels,
        "auto_exit_seconds": args.auto_exit,
        "samples_requested": args.samples,
        "samples_completed": len(samples),
        "command": commands[0],
        "device": next((sample["device"] for sample in samples if sample["device"]), None),
        "publication": {
            "count": len(publications),
            "descriptor_rebind_ms": summary([item["descriptor_rebind_ms"] for item in publications]),
            "resource_swap_ms": summary([item["resource_swap_ms"] for item in publications]),
            "total_publication_ms": summary([item["total_publication_ms"] for item in publications]),
        },
        "frame_render_us": summary(frame_render_us) if frame_render_us else None,
        "samples": samples,
    }
    summary_path = output / "summary.json"
    summary_path.write_text(json.dumps(summary_payload, indent=2) + "\n", encoding="utf-8")
    print(summary_path)
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--samples", type=int, default=3)
    parser.add_argument("--auto-exit", type=float, default=90.0)
    parser.add_argument(
        "--scene",
        choices=("terrain-edits-closed", "radiance-changes", "density-changes"),
        default="terrain-edits-closed",
        help="acceptance scene; terrain-edits-closed performs a real staging publication",
    )
    parser.add_argument("--spacing-voxels", type=int, default=32)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path("target/ddgi-publication-benchmark")
        / datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ"),
    )
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    if args.samples < 1:
        parser.error("--samples must be positive")
    if args.auto_exit <= 0:
        parser.error("--auto-exit must be positive")
    if args.spacing_voxels <= 0:
        parser.error("--spacing-voxels must be positive")
    return args


if __name__ == "__main__":
    raise SystemExit(run(parse_args()))
