#!/usr/bin/env python3
"""Run and compare fixed-camera temporal denoiser benchmarks."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ISOLATION_FLAGS = (
    "--no-clouds",
    "--no-flora",
    "--no-particles",
    "--no-god-rays",
    "--no-lens-flare",
)
METRICS = (
    "mean_abs_luma_delta_8bit",
    "mean_p95_abs_luma_delta_8bit",
    "mean_p99_abs_luma_delta_8bit",
    "mean_noticeable_pixel_ratio",
    "max_transition_mean_abs_luma_delta_8bit",
)


def load_report(path: Path) -> dict:
    report: dict[str, object] = {"aggregate": {}}
    section = "root"
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("["):
            section = "aggregate" if line == "[aggregate]" else "other"
            continue
        if section not in ("root", "aggregate") or "=" not in line:
            continue
        key, value = (part.strip() for part in line.split("=", 1))
        try:
            parsed_value: object = float(value) if any(c in value for c in ".eE") else int(value)
        except ValueError:
            continue
        if section == "aggregate":
            aggregate = report["aggregate"]
            assert isinstance(aggregate, dict)
            aggregate[key] = parsed_value
        else:
            report[key] = parsed_value
    return report


def print_report(path: Path) -> None:
    report = load_report(path)
    aggregate = report["aggregate"]
    print(
        f"{path}: {report['width']}x{report['height']}, "
        f"{report['captured_frames']} frames, {report['transition_count']} transitions"
    )
    for metric in METRICS:
        print(f"  {metric}: {aggregate[metric]:.6f}")


def run_benchmark(args: argparse.Namespace) -> int:
    report = args.report.resolve()
    command = [
        "cargo",
        "run",
        "--release",
        "--",
        "--hidden",
        "--mute",
        "--windowed",
        *DEFAULT_ISOLATION_FLAGS,
        "--denoiser-bench",
        args.preset,
        str(report),
        "--denoiser-bench-warmup-frames",
        str(args.warmup_frames),
        "--denoiser-bench-frames",
        str(args.frames),
    ]
    if args.no_denoise:
        command.append("--no-denoise")
    print("Running:", " ".join(command), flush=True)
    subprocess.run(command, cwd=REPO_ROOT, check=True)
    print_report(report)
    return 0


def compare_reports(args: argparse.Namespace) -> int:
    baseline = load_report(args.baseline)
    candidate = load_report(args.candidate)
    for field in ("width", "height", "captured_frames", "transition_count"):
        if baseline[field] != candidate[field]:
            raise SystemExit(
                f"incompatible reports: {field} is {baseline[field]} vs {candidate[field]}"
            )

    baseline_metrics = baseline["aggregate"]
    candidate_metrics = candidate["aggregate"]
    print(f"{'metric':48} {'baseline':>12} {'candidate':>12} {'change':>10}")
    for metric in METRICS:
        before = float(baseline_metrics[metric])
        after = float(candidate_metrics[metric])
        change = ((after / before) - 1.0) * 100.0 if before else 0.0
        print(f"{metric:48} {before:12.6f} {after:12.6f} {change:+9.2f}%")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    run_parser = subparsers.add_parser("run", help="run one release-mode hidden benchmark")
    run_parser.add_argument("--report", type=Path, required=True)
    run_parser.add_argument("--preset", default="player-default")
    run_parser.add_argument("--warmup-frames", type=int, default=90)
    run_parser.add_argument("--frames", type=int, default=64)
    run_parser.add_argument(
        "--no-denoise",
        action="store_true",
        help="disable the denoiser for an unfiltered reference run",
    )
    run_parser.set_defaults(func=run_benchmark)

    compare_parser = subparsers.add_parser("compare", help="compare two TOML reports")
    compare_parser.add_argument("baseline", type=Path)
    compare_parser.add_argument("candidate", type=Path)
    compare_parser.set_defaults(func=compare_reports)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    if getattr(args, "frames", 2) < 2:
        raise SystemExit("--frames must be at least 2")
    if getattr(args, "warmup_frames", 0) < 0:
        raise SystemExit("--warmup-frames must be non-negative")
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
