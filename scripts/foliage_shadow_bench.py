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
STRUCTURE_METRICS = (
    "midband_spatial_contrast_8bit",
    "significant_hole_fraction",
    "silhouette_edge_density",
    "midband_temporal_delta_8bit",
    "coarse_temporal_delta_8bit",
    "highband_temporal_delta_8bit",
)
STRUCTURE_FINE_TILE_SAMPLES = 4
STRUCTURE_COARSE_TILE_SAMPLES = 16
ACTIVE_SHADOW_DARKENING_8BIT = 0.75


def report_path(prefix: Path, variant: str) -> Path:
    return prefix.with_name(f"{prefix.name}.{variant}.toml")


def load_pair(prefix: Path) -> tuple[dict, dict]:
    shadow = load_report(report_path(prefix, "shadow"))
    control = load_report(report_path(prefix, "control"))
    for field in (
        "scene",
        "source_width",
        "source_height",
        "width",
        "height",
        "captured_frames",
        "structure_sample_scale",
        "structure_sample_width",
        "structure_sample_height",
    ):
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


def load_structure_luma_sequence(report: dict) -> tuple[bytes, int, int, int]:
    path = Path(str(report["structure_luma_sequence_path"]))
    samples = path.read_bytes()
    frame_bytes = int(report["structure_luma_frame_bytes"])
    frame_count = int(report["captured_frames"])
    expected = frame_bytes * frame_count
    if len(samples) != expected:
        raise SystemExit(
            f"invalid structure luma sequence {path}: {len(samples)} bytes, "
            f"expected {expected}"
        )
    width = int(report["structure_sample_width"])
    height = int(report["structure_sample_height"])
    if frame_bytes != width * height:
        raise SystemExit(
            f"invalid structure sample shape {width}x{height} for {frame_bytes} bytes"
        )
    if width % STRUCTURE_COARSE_TILE_SAMPLES or height % STRUCTURE_COARSE_TILE_SAMPLES:
        raise SystemExit(
            f"structure sample shape {width}x{height} is not divisible by "
            f"{STRUCTURE_COARSE_TILE_SAMPLES}"
        )
    return samples, width, height, frame_count


def mean_sequence_frame(samples: bytes, frame_bytes: int, frame_count: int) -> list[float]:
    sums = [0] * frame_bytes
    view = memoryview(samples)
    for frame in range(frame_count):
        start = frame * frame_bytes
        for index, value in enumerate(view[start : start + frame_bytes]):
            sums[index] += value
    return [value / frame_count for value in sums]


def structure_frame_fields(
    frame: memoryview,
    background: list[float],
    width: int,
    height: int,
) -> tuple[
    list[float],
    list[float],
    list[float],
    list[bool],
    float,
    float,
    float,
]:
    signal = [max(0.0, background[index] - value) for index, value in enumerate(frame)]
    fine_tile = STRUCTURE_FINE_TILE_SAMPLES
    coarse_tile = STRUCTURE_COARSE_TILE_SAMPLES
    fine_width = width // fine_tile
    fine_height = height // fine_tile
    coarse_width = width // coarse_tile
    coarse_height = height // coarse_tile
    fine_area = fine_tile * fine_tile
    fine_means = [0.0] * (fine_width * fine_height)
    for fine_y in range(fine_height):
        for fine_x in range(fine_width):
            total = 0.0
            for local_y in range(fine_tile):
                row = (fine_y * fine_tile + local_y) * width + fine_x * fine_tile
                total += sum(signal[row : row + fine_tile])
            fine_means[fine_y * fine_width + fine_x] = total / fine_area

    fine_per_coarse = coarse_tile // fine_tile
    coarse_means = [0.0] * (coarse_width * coarse_height)
    coarse_active = [False] * len(coarse_means)
    for coarse_y in range(coarse_height):
        for coarse_x in range(coarse_width):
            total = 0.0
            for local_y in range(fine_per_coarse):
                row = (
                    (coarse_y * fine_per_coarse + local_y) * fine_width
                    + coarse_x * fine_per_coarse
                )
                total += sum(fine_means[row : row + fine_per_coarse])
            coarse_index = coarse_y * coarse_width + coarse_x
            coarse_means[coarse_index] = total / (fine_per_coarse * fine_per_coarse)
            coarse_active[coarse_index] = (
                coarse_means[coarse_index] >= ACTIVE_SHADOW_DARKENING_8BIT
            )

    midband = [0.0] * len(fine_means)
    highband = [0.0] * len(signal)
    contrast_sum = 0.0
    hole_count = 0
    active_fine_count = 0
    for fine_y in range(fine_height):
        for fine_x in range(fine_width):
            fine_index = fine_y * fine_width + fine_x
            coarse_index = (
                (fine_y // fine_per_coarse) * coarse_width
                + fine_x // fine_per_coarse
            )
            if not coarse_active[coarse_index]:
                continue
            coarse_mean = coarse_means[coarse_index]
            residual = fine_means[fine_index] - coarse_mean
            midband[fine_index] = residual
            contrast_sum += abs(residual)
            hole_count += fine_means[fine_index] <= coarse_mean * 0.25
            active_fine_count += 1

    for y in range(height):
        for x in range(width):
            index = y * width + x
            fine_index = (y // fine_tile) * fine_width + x // fine_tile
            coarse_index = (y // coarse_tile) * coarse_width + x // coarse_tile
            if coarse_active[coarse_index]:
                highband[index] = signal[index] - fine_means[fine_index]

    edge_count = 0
    active_edge_count = 0
    for fine_y in range(fine_height):
        for fine_x in range(fine_width):
            fine_index = fine_y * fine_width + fine_x
            coarse_index = (
                (fine_y // fine_per_coarse) * coarse_width
                + fine_x // fine_per_coarse
            )
            threshold = max(
                ACTIVE_SHADOW_DARKENING_8BIT, coarse_means[coarse_index] * 0.5
            )
            covered = fine_means[fine_index] >= threshold
            if fine_x + 1 < fine_width:
                other_coarse = (
                    (fine_y // fine_per_coarse) * coarse_width
                    + (fine_x + 1) // fine_per_coarse
                )
                if coarse_active[coarse_index] or coarse_active[other_coarse]:
                    other_threshold = max(
                        ACTIVE_SHADOW_DARKENING_8BIT,
                        coarse_means[other_coarse] * 0.5,
                    )
                    edge_count += covered != (
                        fine_means[fine_index + 1] >= other_threshold
                    )
                    active_edge_count += 1
            if fine_y + 1 < fine_height:
                other_coarse = (
                    ((fine_y + 1) // fine_per_coarse) * coarse_width
                    + fine_x // fine_per_coarse
                )
                if coarse_active[coarse_index] or coarse_active[other_coarse]:
                    other_threshold = max(
                        ACTIVE_SHADOW_DARKENING_8BIT,
                        coarse_means[other_coarse] * 0.5,
                    )
                    edge_count += covered != (
                        fine_means[fine_index + fine_width] >= other_threshold
                    )
                    active_edge_count += 1

    return (
        highband,
        midband,
        coarse_means,
        coarse_active,
        contrast_sum / max(1, active_fine_count),
        hole_count / max(1, active_fine_count),
        edge_count / max(1, active_edge_count),
    )


def structure_metrics(shadow: dict, control: dict) -> dict[str, float]:
    shadow_samples, width, height, frame_count = load_structure_luma_sequence(shadow)
    control_samples, control_width, control_height, control_frames = (
        load_structure_luma_sequence(control)
    )
    if (width, height, frame_count) != (control_width, control_height, control_frames):
        raise SystemExit("shadow/control structure sequences have incompatible shapes")
    frame_bytes = width * height
    background = mean_sequence_frame(control_samples, frame_bytes, frame_count)
    view = memoryview(shadow_samples)
    previous_highband: list[float] | None = None
    previous_midband: list[float] | None = None
    previous_coarse: list[float] | None = None
    previous_active: list[bool] | None = None
    spatial_contrast = 0.0
    hole_fraction = 0.0
    edge_density = 0.0
    midband_temporal_sum = 0.0
    midband_temporal_count = 0
    coarse_temporal_sum = 0.0
    coarse_temporal_count = 0
    highband_temporal_sum = 0.0
    highband_temporal_count = 0

    for frame_index in range(frame_count):
        start = frame_index * frame_bytes
        fields = structure_frame_fields(
            view[start : start + frame_bytes], background, width, height
        )
        highband, midband, coarse, active, contrast, holes, edges = fields
        spatial_contrast += contrast
        hole_fraction += holes
        edge_density += edges
        if (
            previous_highband is not None
            and previous_midband is not None
            and previous_coarse is not None
            and previous_active is not None
        ):
            fine_tile = STRUCTURE_FINE_TILE_SAMPLES
            coarse_tile = STRUCTURE_COARSE_TILE_SAMPLES
            fine_width = width // fine_tile
            coarse_width = width // coarse_tile
            fine_per_coarse = coarse_tile // fine_tile
            for index, (before, after) in enumerate(zip(previous_highband, highband)):
                x = index % width
                y = index // width
                coarse_index = (y // coarse_tile) * coarse_width + x // coarse_tile
                if previous_active[coarse_index] or active[coarse_index]:
                    highband_temporal_sum += abs(after - before)
                    highband_temporal_count += 1
            for index, (before, after) in enumerate(zip(previous_midband, midband)):
                fine_x = index % fine_width
                fine_y = index // fine_width
                coarse_index = (
                    (fine_y // fine_per_coarse) * coarse_width
                    + fine_x // fine_per_coarse
                )
                if previous_active[coarse_index] or active[coarse_index]:
                    midband_temporal_sum += abs(after - before)
                    midband_temporal_count += 1
            for coarse_index, (before, after) in enumerate(
                zip(previous_coarse, coarse)
            ):
                if previous_active[coarse_index] or active[coarse_index]:
                    coarse_temporal_sum += abs(after - before)
                    coarse_temporal_count += 1
        previous_highband = highband
        previous_midband = midband
        previous_coarse = coarse
        previous_active = active

    return {
        "midband_spatial_contrast_8bit": spatial_contrast / frame_count,
        "significant_hole_fraction": hole_fraction / frame_count,
        "silhouette_edge_density": edge_density / frame_count,
        "midband_temporal_delta_8bit": midband_temporal_sum
        / max(1, midband_temporal_count),
        "coarse_temporal_delta_8bit": coarse_temporal_sum
        / max(1, coarse_temporal_count),
        "highband_temporal_delta_8bit": highband_temporal_sum
        / max(1, highband_temporal_count),
    }


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
    structure = structure_metrics(shadow, control)
    print("leaf-only structure/motion on bare-terrain shadow receiver:")
    for metric in STRUCTURE_METRICS:
        print(f"  {metric}: {structure[metric]:.6f}")
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
    baseline_structure = structure_metrics(baseline_shadow, baseline_control)
    candidate_structure = structure_metrics(candidate_shadow, candidate_control)
    print(f"{'structure/motion metric':48} {'reference':>16} {'candidate':>17} {'retention':>10}")
    for metric in STRUCTURE_METRICS:
        before = baseline_structure[metric]
        after = candidate_structure[metric]
        retention = after / before if before else 0.0
        print(
            f"{metric:48} {before:16.6f} {after:17.6f} "
            f"{retention * 100.0:9.2f}%"
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
    retention_checks = (
        (
            "midband_spatial_contrast_8bit",
            args.min_midband_spatial_retention,
        ),
        ("significant_hole_fraction", args.min_hole_retention),
        ("silhouette_edge_density", args.min_edge_retention),
        ("midband_temporal_delta_8bit", args.min_motion_retention),
    )
    for metric, limit in retention_checks:
        before = baseline_structure[metric]
        after = candidate_structure[metric]
        retention = after / before if before else 0.0
        if limit is not None and retention < limit:
            failures.append(f"{metric} retention {retention:.3f} < {limit:.3f}")
    if args.max_highband_temporal_growth is not None:
        before = baseline_structure["highband_temporal_delta_8bit"]
        after = candidate_structure["highband_temporal_delta_8bit"]
        growth = after / before - 1.0 if before else 0.0
        if growth > args.max_highband_temporal_growth:
            failures.append(
                f"highband temporal growth {growth:.3f} > "
                f"{args.max_highband_temporal_growth:.3f}"
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
    structure = structure_metrics(shadow, control)
    structure_checks = (
        (
            "midband_spatial_contrast_8bit",
            args.min_midband_spatial_contrast,
        ),
        ("significant_hole_fraction", args.min_significant_hole_fraction),
        ("silhouette_edge_density", args.min_silhouette_edge_density),
        ("midband_temporal_delta_8bit", args.min_midband_temporal_delta),
    )
    for metric, limit in structure_checks:
        value = structure[metric]
        if limit is not None and value < limit:
            failures.append(f"{metric} {value:.6f} < {limit:.6f}")
    highband = structure["highband_temporal_delta_8bit"]
    if (
        args.max_highband_temporal_delta is not None
        and highband > args.max_highband_temporal_delta
    ):
        failures.append(
            f"highband_temporal_delta_8bit {highband:.6f} > "
            f"{args.max_highband_temporal_delta:.6f}"
        )
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
    compare_parser.add_argument("--min-midband-spatial-retention", type=float)
    compare_parser.add_argument("--min-hole-retention", type=float)
    compare_parser.add_argument("--min-edge-retention", type=float)
    compare_parser.add_argument("--min-motion-retention", type=float)
    compare_parser.add_argument("--max-highband-temporal-growth", type=float)
    compare_parser.set_defaults(func=compare_reports)

    check_parser = subparsers.add_parser("check", help="check one paired capture against limits")
    check_parser.add_argument("prefix", type=Path)
    check_parser.add_argument("--max-mean-temporal-excess", type=float)
    check_parser.add_argument("--max-noticeable-temporal-excess", type=float)
    check_parser.add_argument("--min-midband-spatial-contrast", type=float)
    check_parser.add_argument("--min-significant-hole-fraction", type=float)
    check_parser.add_argument("--min-silhouette-edge-density", type=float)
    check_parser.add_argument("--min-midband-temporal-delta", type=float)
    check_parser.add_argument("--max-highband-temporal-delta", type=float)
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
