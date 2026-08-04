#!/usr/bin/env python3
"""Measure secondary diagonal lighting bands in the deterministic patt seam capture."""

from __future__ import annotations

import argparse
import struct
import sys
import zlib
from dataclasses import dataclass
from pathlib import Path


PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
ROI = (0.56, 0.50, 0.643_333_333_3, 0.672_839_506_2)
PROFILE_SLOPE = -0.5
MIN_CONTRAST = 30.0
MIN_PRIMARY_GRADIENT = 0.4
MIN_SECONDARY_BANDS = 2
MIN_SECONDARY_PRIMARY_RATIO = 0.75


@dataclass(frozen=True)
class SeamMetric:
    width: int
    height: int
    roi_pixels: tuple[int, int, int, int]
    contrast: float
    primary_gradient: float
    secondary_band_count: int
    secondary_primary_ratio: float

    @property
    def is_red(self) -> bool:
        return (
            self.contrast >= MIN_CONTRAST
            and self.primary_gradient >= MIN_PRIMARY_GRADIENT
            and self.secondary_band_count >= MIN_SECONDARY_BANDS
            and self.secondary_primary_ratio >= MIN_SECONDARY_PRIMARY_RATIO
        )


def _paeth(left: int, above: int, upper_left: int) -> int:
    prediction = left + above - upper_left
    left_distance = abs(prediction - left)
    above_distance = abs(prediction - above)
    upper_left_distance = abs(prediction - upper_left)
    if left_distance <= above_distance and left_distance <= upper_left_distance:
        return left
    if above_distance <= upper_left_distance:
        return above
    return upper_left


def read_rgba8_png(path: Path) -> tuple[int, int, list[bytearray]]:
    data = path.read_bytes()
    if not data.startswith(PNG_SIGNATURE):
        raise ValueError(f"{path} is not a PNG")

    position = len(PNG_SIGNATURE)
    idat = bytearray()
    header: tuple[int, int, int, int, int] | None = None
    while position < len(data):
        if position + 12 > len(data):
            raise ValueError(f"{path} has a truncated PNG chunk")
        size = struct.unpack_from(">I", data, position)[0]
        chunk_type = data[position + 4 : position + 8]
        chunk_data = data[position + 8 : position + 8 + size]
        if len(chunk_data) != size:
            raise ValueError(f"{path} has a truncated {chunk_type!r} chunk")
        position += size + 12
        if chunk_type == b"IHDR":
            width, height, bit_depth, color_type, compression, filtering, interlace = (
                struct.unpack(">IIBBBBB", chunk_data)
            )
            header = (width, height, bit_depth, color_type, interlace)
            if compression != 0 or filtering != 0:
                raise ValueError(f"{path} uses unsupported PNG compression or filtering")
        elif chunk_type == b"IDAT":
            idat.extend(chunk_data)
        elif chunk_type == b"IEND":
            break

    if header is None:
        raise ValueError(f"{path} has no IHDR chunk")
    width, height, bit_depth, color_type, interlace = header
    if (bit_depth, color_type, interlace) != (8, 6, 0):
        raise ValueError(
            f"{path} must be a non-interlaced 8-bit RGBA PNG; "
            f"got depth={bit_depth} color_type={color_type} interlace={interlace}"
        )

    bytes_per_pixel = 4
    stride = width * bytes_per_pixel
    payload = zlib.decompress(idat)
    expected_size = height * (stride + 1)
    if len(payload) != expected_size:
        raise ValueError(
            f"{path} decoded to {len(payload)} bytes; expected {expected_size}"
        )

    rows: list[bytearray] = []
    previous = bytearray(stride)
    offset = 0
    for _ in range(height):
        filter_type = payload[offset]
        row = bytearray(payload[offset + 1 : offset + stride + 1])
        offset += stride + 1
        if filter_type > 4:
            raise ValueError(f"{path} uses unsupported PNG filter {filter_type}")
        for index in range(stride):
            left = row[index - bytes_per_pixel] if index >= bytes_per_pixel else 0
            above = previous[index]
            upper_left = previous[index - bytes_per_pixel] if index >= bytes_per_pixel else 0
            if filter_type == 1:
                row[index] = (row[index] + left) & 0xFF
            elif filter_type == 2:
                row[index] = (row[index] + above) & 0xFF
            elif filter_type == 3:
                row[index] = (row[index] + ((left + above) // 2)) & 0xFF
            elif filter_type == 4:
                row[index] = (row[index] + _paeth(left, above, upper_left)) & 0xFF
        rows.append(row)
        previous = row
    return width, height, rows


def _luminance(row: bytearray, x: int) -> float:
    offset = x * 4
    red, green, blue = row[offset : offset + 3]
    return (54.0 * red + 183.0 * green + 19.0 * blue) / 256.0


def _box_filter(values: list[float], radius: int) -> list[float]:
    prefix = [0.0]
    for value in values:
        prefix.append(prefix[-1] + value)
    result = []
    for index in range(len(values)):
        start = max(0, index - radius)
        end = min(len(values), index + radius + 1)
        result.append((prefix[end] - prefix[start]) / (end - start))
    return result


def measure(path: Path) -> SeamMetric:
    width, height, rows = read_rgba8_png(path)
    x0 = round(width * ROI[0])
    y0 = round(height * ROI[1])
    x1 = round(width * ROI[2])
    y1 = round(height * ROI[3])
    if x1 - x0 < 32 or y1 - y0 < 32:
        raise ValueError(f"{path} is too small for the patt seam ROI")

    roi_width = x1 - x0
    roi_height = y1 - y0
    diagonal_bins: dict[int, list[float]] = {}
    for local_y, row in enumerate(rows[y0:y1]):
        for local_x in range(roi_width):
            diagonal = round(local_y + PROFILE_SLOPE * local_x)
            total_and_count = diagonal_bins.setdefault(diagonal, [0.0, 0.0])
            total_and_count[0] += _luminance(row, x0 + local_x)
            total_and_count[1] += 1.0

    minimum_coverage = roi_width * 0.65
    profile = [
        (diagonal, total / count)
        for diagonal, (total, count) in sorted(diagonal_bins.items())
        if count >= minimum_coverage
    ]
    if len(profile) < 32:
        raise ValueError(f"{path} has insufficient covered diagonal samples")

    coordinates = [coordinate for coordinate, _ in profile]
    values = [value for _, value in profile]
    scale = height / 1620.0
    smooth_radius = max(1, round(3 * scale))
    gradient_step = max(1, round(4 * scale))
    peak_radius = max(1, round(5 * scale))
    edge_exclusion = max(5, round(10 * scale))
    smoothed = _box_filter(values, smooth_radius)
    gradients = [0.0] * len(smoothed)
    for index in range(gradient_step, len(smoothed) - gradient_step):
        gradients[index] = (
            smoothed[index + gradient_step] - smoothed[index - gradient_step]
        ) / (2 * gradient_step)

    candidates: list[tuple[float, int]] = []
    border = smooth_radius + peak_radius + gradient_step
    for index in range(border, len(gradients) - border):
        gradient = gradients[index]
        neighborhood = gradients[index - peak_radius : index + peak_radius + 1]
        if gradient >= 0.1 and gradient >= max(neighborhood):
            candidates.append((gradient, coordinates[index]))
    if not candidates:
        return SeamMetric(
            width,
            height,
            (x0, y0, x1, y1),
            max(smoothed) - min(smoothed),
            0.0,
            0,
            0.0,
        )

    candidates.sort(reverse=True)
    separated_candidates: list[tuple[float, int]] = []
    for candidate in candidates:
        if all(
            abs(candidate[1] - selected[1]) >= edge_exclusion
            for selected in separated_candidates
        ):
            separated_candidates.append(candidate)

    primary_gradient, primary_coordinate = separated_candidates[0]
    secondary = [
        candidate
        for candidate in separated_candidates[1:]
        if abs(candidate[1] - primary_coordinate) >= edge_exclusion
        and candidate[0] >= primary_gradient * 0.2
    ]
    secondary_energy = sum(gradient for gradient, _ in secondary[:5])
    return SeamMetric(
        width,
        height,
        (x0, y0, x1, y1),
        max(smoothed) - min(smoothed),
        primary_gradient,
        len(secondary),
        secondary_energy / primary_gradient,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("screenshot", type=Path)
    args = parser.parse_args()
    try:
        metric = measure(args.screenshot)
    except (OSError, ValueError, zlib.error) as error:
        print(f"[PATT_DDGI_SEAM] verdict=ERROR reason={error}", file=sys.stderr)
        return 2

    verdict = "RED" if metric.is_red else "GREEN"
    roi = ",".join(str(value) for value in metric.roi_pixels)
    print(
        f"[PATT_DDGI_SEAM] verdict={verdict} screenshot={args.screenshot} "
        f"size={metric.width}x{metric.height} roi={roi} "
        f"contrast={metric.contrast:.3f} primary_gradient={metric.primary_gradient:.6f} "
        f"primary_edge_excluded=true secondary_bands={metric.secondary_band_count} "
        f"secondary_primary_ratio={metric.secondary_primary_ratio:.6f} "
        f"thresholds=contrast:{MIN_CONTRAST:.1f},primary:{MIN_PRIMARY_GRADIENT:.1f},"
        f"bands:{MIN_SECONDARY_BANDS},ratio:{MIN_SECONDARY_PRIMARY_RATIO:.2f}"
    )
    return 1 if metric.is_red else 0


if __name__ == "__main__":
    raise SystemExit(main())
