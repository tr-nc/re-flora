#!/usr/bin/env python3
"""Measure internal saved-terrain DDGI bands after excluding the hard light edge."""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path

from analyze_patt_ddgi_seam import read_rgba8_png


# The repro runner's 55%x68% center crop is 1584x1102. Keep the ROI normalized so the
# analyzer can also inspect an uncropped 2880x1620 screenshot from the same camera.
ROI = (700 / 1584, 80 / 1102, 1500 / 1584, 1000 / 1102)
SMOOTH_RADIUS = 3
GRADIENT_STEP = 4
PEAK_RADIUS = 10
EDGE_EXCLUSION = 50
PEAK_SEPARATION = 20
MIN_INTERNAL_GRADIENT = 0.03
MIN_PROFILE_CONTRAST = 10.0
MIN_INTERNAL_BANDS = 2
MIN_INTERNAL_PRIMARY_RATIO = 0.20


@dataclass(frozen=True)
class SavedDdgiSeamMetric:
    width: int
    height: int
    roi_pixels: tuple[int, int, int, int]
    profile_contrast: float
    primary_edge_row: int
    primary_gradient: float
    internal_band_count: int
    internal_primary_ratio: float

    @property
    def is_red(self) -> bool:
        return (
            self.profile_contrast >= MIN_PROFILE_CONTRAST
            and self.internal_band_count >= MIN_INTERNAL_BANDS
            and self.internal_primary_ratio >= MIN_INTERNAL_PRIMARY_RATIO
        )


def _luminance(row: bytearray, x: int) -> float:
    offset = x * 4
    red, green, blue = row[offset : offset + 3]
    return (54.0 * red + 183.0 * green + 19.0 * blue) / 256.0


def _smooth(values: list[float]) -> list[float]:
    result = []
    for index in range(len(values)):
        start = max(0, index - SMOOTH_RADIUS)
        end = min(len(values), index + SMOOTH_RADIUS + 1)
        result.append(sum(values[start:end]) / (end - start))
    return result


def measure(path: Path) -> SavedDdgiSeamMetric:
    width, height, rows = read_rgba8_png(path)
    x0 = round(width * ROI[0])
    y0 = round(height * ROI[1])
    x1 = round(width * ROI[2])
    y1 = round(height * ROI[3])
    if x1 - x0 < 32 or y1 - y0 < 64:
        raise ValueError(f"{path} is too small for the saved DDGI seam ROI")

    profile: list[float] = []
    for y in range(y0, y1):
        values = []
        for x in range(x0, x1):
            red = rows[y][4 * x]
            green = rows[y][4 * x + 1]
            blue = rows[y][4 * x + 2]
            # Depth-tested probe diamonds and their white crosshair are overlays, not wall
            # irradiance. Exclude only vivid green markers; the neutral wall remains measured.
            if green > 120 and green > red * 1.3 and green > blue * 1.2:
                continue
            values.append(_luminance(rows[y], x))
        if not values:
            raise ValueError(f"{path} has no non-overlay pixels in row {y}")
        profile.append(sum(values) / len(values))

    smoothed = _smooth(profile)
    gradients = [0.0] * len(smoothed)
    for index in range(GRADIENT_STEP, len(smoothed) - GRADIENT_STEP):
        gradients[index] = (
            smoothed[index + GRADIENT_STEP] - smoothed[index - GRADIENT_STEP]
        ) / (2 * GRADIENT_STEP)

    edge_border = PEAK_RADIUS + GRADIENT_STEP
    edge_index = max(
        range(edge_border, len(gradients) - edge_border),
        key=lambda index: abs(gradients[index]),
    )
    primary_gradient = abs(gradients[edge_index])
    candidates: list[tuple[float, int]] = []
    for index in range(edge_border, len(gradients) - edge_border):
        if abs(index - edge_index) < EDGE_EXCLUSION:
            continue
        gradient = abs(gradients[index])
        if gradient < MIN_INTERNAL_GRADIENT:
            continue
        neighborhood = gradients[index - PEAK_RADIUS : index + PEAK_RADIUS + 1]
        if gradient >= max(abs(value) for value in neighborhood):
            candidates.append((gradient, index))

    separated: list[tuple[float, int]] = []
    for candidate in sorted(candidates, reverse=True):
        if all(abs(candidate[1] - selected[1]) >= PEAK_SEPARATION for selected in separated):
            separated.append(candidate)
    internal_energy = sum(gradient for gradient, _ in separated[:5])
    ratio = internal_energy / primary_gradient if primary_gradient > 1.0e-8 else 0.0

    return SavedDdgiSeamMetric(
        width=width,
        height=height,
        roi_pixels=(x0, y0, x1, y1),
        profile_contrast=max(smoothed) - min(smoothed),
        primary_edge_row=y0 + edge_index,
        primary_gradient=primary_gradient,
        internal_band_count=len(separated),
        internal_primary_ratio=ratio,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("screenshot", type=Path, help="saved-terrain center crop or full screenshot")
    args = parser.parse_args()
    try:
        metric = measure(args.screenshot)
    except (OSError, ValueError, UnicodeError, EOFError) as error:
        print(f"[SAVED_DDGI_SEAM] verdict=ERROR reason={error}", file=sys.stderr)
        return 2

    verdict = "RED" if metric.is_red else "GREEN"
    x0, y0, x1, y1 = metric.roi_pixels
    print(
        f"[SAVED_DDGI_SEAM] verdict={verdict} screenshot={args.screenshot} "
        f"size={metric.width}x{metric.height} roi={x0},{y0},{x1},{y1} "
        f"profile_contrast={metric.profile_contrast:.3f} "
        f"primary_edge_row={metric.primary_edge_row} "
        f"primary_gradient={metric.primary_gradient:.6f} "
        f"edge_excluded=true internal_bands={metric.internal_band_count} "
        f"internal_primary_ratio={metric.internal_primary_ratio:.6f} "
        f"thresholds=contrast:{MIN_PROFILE_CONTRAST:.1f},bands:{MIN_INTERNAL_BANDS},"
        f"ratio:{MIN_INTERNAL_PRIMARY_RATIO:.2f}"
    )
    return 1 if metric.is_red else 0


if __name__ == "__main__":
    raise SystemExit(main())
