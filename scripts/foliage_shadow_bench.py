#!/usr/bin/env python3
"""Run and evaluate the deterministic moving foliage-shadow receiver benchmark."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

from denoiser_bench import DETAIL_METRIC, load_report


REPO_ROOT = Path(__file__).resolve().parents[1]
ISOLATION_FLAGS = (
    "--no-clouds",
    "--no-particles",
    "--no-god-rays",
    "--no-lens-flare",
)
TEMPORAL_METRICS = (
    "mean_abs_luma_delta_8bit",
    "mean_p99_abs_luma_delta_8bit",
    "mean_noticeable_pixel_ratio",
    "max_transition_mean_abs_luma_delta_8bit",
)
EXCESS_METRICS = (
    "mean_abs_luma_delta_8bit",
    "mean_p99_abs_luma_delta_8bit",
    "mean_noticeable_pixel_ratio",
)


def report_path(prefix: Path, variant: str) -> Path:
    return prefix.with_name(f"{prefix.name}.{variant}.toml")


def load_pair(prefix: Path) -> tuple[dict, dict]:
    shadow = load_report(report_path(prefix, "shadow"))
    control = load_report(report_path(prefix, "control"))
    for field in ("scene", "source_width", "source_height", "width", "height", "captured_frames"):
        if shadow.get(field) != control.get(field):
            raise SystemExit(
                f"incompatible shadow/control reports: {field} is "
                f"{shadow.get(field)!r} vs {control.get(field)!r}"
            )
    if shadow.get("scene") != "foliage-shadow":
        raise SystemExit(f"expected foliage-shadow reports, got {shadow.get('scene')!r}")
    return shadow, control


def aggregate(report: dict) -> dict:
    value = report["aggregate"]
    assert isinstance(value, dict)
    return value


def load_luma_sequence(report: dict) -> bytes:
    path = Path(str(report["luma_sequence_path"]))
    samples = path.read_bytes()
    expected = int(report["luma_frame_bytes"]) * int(report["captured_frames"])
    if len(samples) != expected:
        raise SystemExit(
            f"invalid luma sequence {path}: {len(samples)} bytes, expected {expected}"
        )
    return samples


def temporal_excess_metrics(shadow: dict, control: dict) -> dict[str, float]:
    """Subtract paired run distributions without assuming cross-process phase alignment."""
    return {
        metric: max(0.0, float(aggregate(shadow)[metric]) - float(aggregate(control)[metric]))
        for metric in EXCESS_METRICS
    }


def mean_luma(report: dict) -> float:
    samples = load_luma_sequence(report)
    return sum(samples) / len(samples)


def leaf_darkening(shadow: dict, control: dict) -> float:
    return max(0.0, mean_luma(control) - mean_luma(shadow))


def print_pair(prefix: Path) -> None:
    shadow, control = load_pair(prefix)
    print(
        f"{prefix}: scene={shadow['scene']}, source={shadow['source_width']}x{shadow['source_height']}, "
        f"receiver_roi={shadow['width']}x{shadow['height']}+{shadow['analysis_x']},{shadow['analysis_y']}, "
        f"frames={shadow['captured_frames']}"
    )
    print(f"{'raw receiver metric':48} {'shadow':>12} {'control':>12}")
    for metric in TEMPORAL_METRICS:
        shadow_value = float(aggregate(shadow)[metric])
        control_value = float(aggregate(control)[metric])
        print(f"{metric:48} {shadow_value:12.6f} {control_value:12.6f}")
    excess = temporal_excess_metrics(shadow, control)
    print("leaf-shadow temporal excess (shadow distribution - control distribution):")
    for metric in EXCESS_METRICS:
        print(f"  {metric}: {excess[metric]:.6f}")
    print(f"mean_leaf_darkening_8bit: {leaf_darkening(shadow, control):.6f}")
    print(
        f"{DETAIL_METRIC:48} {float(aggregate(shadow)[DETAIL_METRIC]):12.6f} "
        f"{float(aggregate(control)[DETAIL_METRIC]):12.6f}"
    )
    print(
        f"capture_seconds: shadow={float(shadow['capture_seconds']):.4f}, "
        f"control={float(control['capture_seconds']):.4f}"
    )


def run_variant(args: argparse.Namespace, prefix: Path, variant: str) -> None:
    path = report_path(prefix, variant).resolve()
    command = [
        "cargo",
        "run",
        "--release",
        "--",
        "--hidden",
        "--mute",
        "--windowed",
        *ISOLATION_FLAGS,
        "--foliage-shadow-bench",
        str(path),
        "--foliage-shadow-bench-warmup-frames",
        str(args.warmup_frames),
        "--foliage-shadow-bench-frames",
        str(args.frames),
    ]
    if variant == "control":
        command.append("--no-leaf-shadows")
    print("Running:", " ".join(command), flush=True)
    subprocess.run(command, cwd=REPO_ROOT, check=True)


def run_benchmark(args: argparse.Namespace) -> int:
    prefix = args.prefix.resolve()
    prefix.parent.mkdir(parents=True, exist_ok=True)
    for variant in ("shadow", "control"):
        run_variant(args, prefix, variant)
    print_pair(prefix)
    return 0


def compare_reports(args: argparse.Namespace) -> int:
    baseline_shadow, baseline_control = load_pair(args.baseline)
    candidate_shadow, candidate_control = load_pair(args.candidate)
    baseline_excess = temporal_excess_metrics(baseline_shadow, baseline_control)
    candidate_excess = temporal_excess_metrics(candidate_shadow, candidate_control)
    print(f"{'temporal excess metric':48} {'baseline':>16} {'candidate':>17} {'change':>10}")
    for metric in EXCESS_METRICS:
        before = baseline_excess[metric]
        after = candidate_excess[metric]
        change = ((after / before) - 1.0) * 100.0 if before else 0.0
        print(f"{metric:48} {before:16.6f} {after:17.6f} {change:+9.2f}%")

    baseline_detail = float(aggregate(baseline_shadow)[DETAIL_METRIC])
    candidate_detail = float(aggregate(candidate_shadow)[DETAIL_METRIC])
    detail_loss = max(0.0, 1.0 - candidate_detail / baseline_detail) if baseline_detail else 0.0
    print(
        f"{DETAIL_METRIC:48} {baseline_detail:16.6f} {candidate_detail:17.6f} "
        f"{-detail_loss * 100.0:+9.2f}%"
    )
    baseline_darkening = leaf_darkening(baseline_shadow, baseline_control)
    candidate_darkening = leaf_darkening(candidate_shadow, candidate_control)
    darkening_change = (
        abs(candidate_darkening / baseline_darkening - 1.0) if baseline_darkening else 0.0
    )
    print(
        f"{'mean_leaf_darkening_8bit':48} {baseline_darkening:16.6f} "
        f"{candidate_darkening:17.6f} "
        f"{((candidate_darkening / baseline_darkening - 1.0) * 100.0 if baseline_darkening else 0.0):+9.2f}%"
    )

    failures = []
    if args.min_mean_excess_reduction is not None:
        before = baseline_excess["mean_abs_luma_delta_8bit"]
        after = candidate_excess["mean_abs_luma_delta_8bit"]
        reduction = 1.0 - after / before if before else 0.0
        if reduction < args.min_mean_excess_reduction:
            failures.append(
                f"mean temporal-excess reduction {reduction:.3f} < "
                f"{args.min_mean_excess_reduction:.3f}"
            )
    if args.max_detail_loss is not None and detail_loss > args.max_detail_loss:
        failures.append(f"detail loss {detail_loss:.3f} > {args.max_detail_loss:.3f}")
    if (
        args.max_leaf_darkening_change is not None
        and darkening_change > args.max_leaf_darkening_change
    ):
        failures.append(
            f"leaf darkening change {darkening_change:.3f} > "
            f"{args.max_leaf_darkening_change:.3f}"
        )
    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    return 0


def check_report(args: argparse.Namespace) -> int:
    shadow, control = load_pair(args.prefix)
    excess = temporal_excess_metrics(shadow, control)
    failures = []
    checks = (
        ("mean_abs_luma_delta_8bit", args.max_mean_temporal_excess),
        ("mean_noticeable_pixel_ratio", args.max_noticeable_temporal_excess),
    )
    for metric, limit in checks:
        value = excess[metric]
        if limit is not None and value > limit:
            failures.append(f"{metric} temporal excess {value:.6f} > {limit:.6f}")
    print_pair(args.prefix)
    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    run_parser = subparsers.add_parser("run", help="run shadow and no-shadow release captures")
    run_parser.add_argument("prefix", type=Path)
    run_parser.add_argument("--warmup-frames", type=int, default=90)
    run_parser.add_argument("--frames", type=int, default=64)
    run_parser.set_defaults(func=run_benchmark)

    compare_parser = subparsers.add_parser("compare", help="compare two paired captures")
    compare_parser.add_argument("baseline", type=Path)
    compare_parser.add_argument("candidate", type=Path)
    compare_parser.add_argument("--min-mean-excess-reduction", type=float)
    compare_parser.add_argument("--max-detail-loss", type=float)
    compare_parser.add_argument("--max-leaf-darkening-change", type=float)
    compare_parser.set_defaults(func=compare_reports)

    check_parser = subparsers.add_parser("check", help="check one paired capture against limits")
    check_parser.add_argument("prefix", type=Path)
    check_parser.add_argument("--max-mean-temporal-excess", type=float)
    check_parser.add_argument("--max-noticeable-temporal-excess", type=float)
    check_parser.set_defaults(func=check_report)
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
