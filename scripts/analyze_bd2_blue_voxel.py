#!/usr/bin/env python3
"""Detect the isolated blue terrain voxel in the bd2 dark-interior snapshot."""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path

from analyze_patt_ddgi_seam import read_rgba8_png


# Normalized so the gate remains valid if the hidden swapchain extent changes. This central
# interior region contains the reported terrain voxel while excluding the colored HUD panels.
ROI = (0.35, 0.25, 0.60, 0.58)
MIN_BLUE = 16
MIN_BLUE_OVER_RED = 8
MIN_BLUE_OVER_GREEN = 4


@dataclass(frozen=True)
class BlueVoxelMetric:
    width: int
    height: int
    roi_pixels: tuple[int, int, int, int]
    blue_pixel_count: int
    blue_bounds: tuple[int, int, int, int] | None
    peak_rgb: tuple[int, int, int]


def _is_blue_artifact(red: int, green: int, blue: int) -> bool:
    return (
        blue >= MIN_BLUE
        and blue - red >= MIN_BLUE_OVER_RED
        and blue - green >= MIN_BLUE_OVER_GREEN
    )


def measure(path: Path) -> BlueVoxelMetric:
    width, height, rows = read_rgba8_png(path)
    x0 = round(width * ROI[0])
    y0 = round(height * ROI[1])
    x1 = round(width * ROI[2])
    y1 = round(height * ROI[3])
    if x1 <= x0 or y1 <= y0:
        raise ValueError(f"{path} is too small for the bd2 artifact ROI")

    count = 0
    min_x = width
    min_y = height
    max_x = -1
    max_y = -1
    peak_rgb = (0, 0, 0)
    peak_blue_excess = -1
    for y in range(y0, y1):
        row = rows[y]
        for x in range(x0, x1):
            offset = x * 4
            red, green, blue = row[offset : offset + 3]
            if not _is_blue_artifact(red, green, blue):
                continue
            count += 1
            min_x = min(min_x, x)
            min_y = min(min_y, y)
            max_x = max(max_x, x)
            max_y = max(max_y, y)
            blue_excess = blue - max(red, green)
            if blue_excess > peak_blue_excess:
                peak_blue_excess = blue_excess
                peak_rgb = (red, green, blue)

    bounds = None if count == 0 else (min_x, min_y, max_x, max_y)
    return BlueVoxelMetric(
        width=width,
        height=height,
        roi_pixels=(x0, y0, x1, y1),
        blue_pixel_count=count,
        blue_bounds=bounds,
        peak_rgb=peak_rgb,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("screenshot", type=Path)
    parser.add_argument("--max-blue-pixels", type=int, default=0)
    args = parser.parse_args()
    if args.max_blue_pixels < 0:
        parser.error("--max-blue-pixels must be non-negative")

    try:
        metric = measure(args.screenshot)
    except (OSError, ValueError, UnicodeError, EOFError) as error:
        print(f"[BD2_BLUE_VOXEL] verdict=ERROR reason={error}", file=sys.stderr)
        return 2

    is_red = metric.blue_pixel_count > args.max_blue_pixels
    verdict = "RED" if is_red else "GREEN"
    x0, y0, x1, y1 = metric.roi_pixels
    bounds = "none" if metric.blue_bounds is None else ",".join(map(str, metric.blue_bounds))
    print(
        f"[BD2_BLUE_VOXEL] verdict={verdict} screenshot={args.screenshot} "
        f"size={metric.width}x{metric.height} roi={x0},{y0},{x1},{y1} "
        f"blue_pixels={metric.blue_pixel_count} max_blue_pixels={args.max_blue_pixels} "
        f"bounds={bounds} peak_rgb={metric.peak_rgb[0]},{metric.peak_rgb[1]},{metric.peak_rgb[2]} "
        f"thresholds=blue:{MIN_BLUE},over_red:{MIN_BLUE_OVER_RED},"
        f"over_green:{MIN_BLUE_OVER_GREEN}"
    )
    return 1 if is_red else 0


if __name__ == "__main__":
    raise SystemExit(main())
