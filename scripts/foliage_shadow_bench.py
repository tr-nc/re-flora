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
LEAF_SIGNAL_METRICS = (
    "mean_abs_luma_delta_8bit",
    "mean_p99_abs_luma_delta_8bit",
    "mean_noticeable_pixel_ratio",
    "max_transition_mean_abs_luma_delta_8bit",
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


def histogram_percentile(histogram: list[int], sample_count: int, percentile: float) -> int:
    target = max(1, int(sample_count * percentile + 0.999999))
    cumulative = 0
    for value, count in enumerate(histogram):
        cumulative += count
        if cumulative >= target:
            return value
    return len(histogram) - 1


def leaf_signal_metrics(shadow: dict, control: dict) -> dict[str, float]:
    shadow_luma = load_luma_sequence(shadow)
    control_luma = load_luma_sequence(control)
    if len(shadow_luma) != len(control_luma):
        raise SystemExit("shadow/control luma sequences have different sizes")

    frame_bytes = int(shadow["luma_frame_bytes"])
    frame_count = int(shadow["captured_frames"])
    threshold = int(shadow["noticeable_delta_threshold_8bit"])
    previous_signal = [
        control_luma[index] - shadow_luma[index] for index in range(frame_bytes)
    ]
    transition_means = []
    transition_p99 = []
    transition_noticeable = []
    max_delta = 0
    for frame in range(1, frame_count):
        start = frame * frame_bytes
        histogram = [0] * 511
        delta_sum = 0
        noticeable = 0
        for pixel in range(frame_bytes):
            index = start + pixel
            signal = control_luma[index] - shadow_luma[index]
            delta = abs(signal - previous_signal[pixel])
            previous_signal[pixel] = signal
            histogram[delta] += 1
            delta_sum += delta
            noticeable += delta >= threshold
            max_delta = max(max_delta, delta)
        transition_means.append(delta_sum / frame_bytes)
        transition_p99.append(histogram_percentile(histogram, frame_bytes, 0.99))
        transition_noticeable.append(noticeable / frame_bytes)

    transition_count = max(1, len(transition_means))
    return {
        "mean_abs_luma_delta_8bit": sum(transition_means) / transition_count,
        "mean_p99_abs_luma_delta_8bit": sum(transition_p99) / transition_count,
        "mean_noticeable_pixel_ratio": sum(transition_noticeable) / transition_count,
        "max_transition_mean_abs_luma_delta_8bit": max(transition_means, default=0.0),
        "max_abs_luma_delta_8bit": float(max_delta),
    }


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
    signal = leaf_signal_metrics(shadow, control)
    print("leaf shadow signal temporal metrics (control_luma - shadow_luma):")
    for metric in LEAF_SIGNAL_METRICS:
        print(f"  {metric}: {signal[metric]:.6f}")
    print(f"  max_abs_luma_delta_8bit: {signal['max_abs_luma_delta_8bit']:.0f}")
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
    baseline_signal = leaf_signal_metrics(baseline_shadow, baseline_control)
    candidate_signal = leaf_signal_metrics(candidate_shadow, candidate_control)
    print(f"{'leaf signal metric':48} {'baseline':>16} {'candidate':>17} {'change':>10}")
    for metric in LEAF_SIGNAL_METRICS:
        before = baseline_signal[metric]
        after = candidate_signal[metric]
        change = ((after / before) - 1.0) * 100.0 if before else 0.0
        print(f"{metric:48} {before:16.6f} {after:17.6f} {change:+9.2f}%")

    baseline_detail = float(aggregate(baseline_shadow)[DETAIL_METRIC])
    candidate_detail = float(aggregate(candidate_shadow)[DETAIL_METRIC])
    detail_loss = max(0.0, 1.0 - candidate_detail / baseline_detail) if baseline_detail else 0.0
    print(
        f"{DETAIL_METRIC:48} {baseline_detail:16.6f} {candidate_detail:17.6f} "
        f"{-detail_loss * 100.0:+9.2f}%"
    )

    failures = []
    if args.min_mean_signal_reduction is not None:
        before = baseline_signal["mean_abs_luma_delta_8bit"]
        after = candidate_signal["mean_abs_luma_delta_8bit"]
        reduction = 1.0 - after / before if before else 0.0
        if reduction < args.min_mean_signal_reduction:
            failures.append(
                f"mean leaf-signal reduction {reduction:.3f} < "
                f"{args.min_mean_signal_reduction:.3f}"
            )
    if args.max_detail_loss is not None and detail_loss > args.max_detail_loss:
        failures.append(f"detail loss {detail_loss:.3f} > {args.max_detail_loss:.3f}")
    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    return 0


def check_report(args: argparse.Namespace) -> int:
    shadow, control = load_pair(args.prefix)
    signal = leaf_signal_metrics(shadow, control)
    failures = []
    checks = (
        ("mean_abs_luma_delta_8bit", args.max_mean_signal_delta),
        ("mean_noticeable_pixel_ratio", args.max_noticeable_signal_ratio),
    )
    for metric, limit in checks:
        value = signal[metric]
        if limit is not None and value > limit:
            failures.append(f"{metric} leaf signal {value:.6f} > {limit:.6f}")
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
    compare_parser.add_argument("--min-mean-signal-reduction", type=float)
    compare_parser.add_argument("--max-detail-loss", type=float)
    compare_parser.set_defaults(func=compare_reports)

    check_parser = subparsers.add_parser("check", help="check one paired capture against limits")
    check_parser.add_argument("prefix", type=Path)
    check_parser.add_argument("--max-mean-signal-delta", type=float)
    check_parser.add_argument("--max-noticeable-signal-ratio", type=float)
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
