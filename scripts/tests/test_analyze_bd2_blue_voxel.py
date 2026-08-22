from __future__ import annotations

import binascii
import struct
import sys
import tempfile
import unittest
import zlib
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import analyze_bd2_blue_voxel as analyzer  # noqa: E402
import analyze_patt_ddgi_seam as png  # noqa: E402


def write_png(path: Path, pixels: list[list[tuple[int, int, int]]]) -> None:
    height = len(pixels)
    width = len(pixels[0])

    def chunk(kind: bytes, payload: bytes) -> bytes:
        return (
            struct.pack(">I", len(payload))
            + kind
            + payload
            + struct.pack(">I", binascii.crc32(kind + payload) & 0xFFFF_FFFF)
        )

    raw = bytearray()
    for row in pixels:
        raw.append(0)
        for red, green, blue in row:
            raw.extend((red, green, blue, 255))
    path.write_bytes(
        png.PNG_SIGNATURE
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


class Bd2BlueVoxelAnalyzerTests(unittest.TestCase):
    def test_dark_dither_is_green(self) -> None:
        pixels = [[(1, 1, 2) for _ in range(100)] for _ in range(100)]
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "dark.png"
            write_png(path, pixels)
            metric = analyzer.measure(path)

        self.assertEqual(metric.blue_pixel_count, 0)
        self.assertIsNone(metric.blue_bounds)

    def test_isolated_blue_voxel_in_interior_is_red(self) -> None:
        pixels = [[(0, 0, 0) for _ in range(100)] for _ in range(100)]
        for y in range(40, 43):
            for x in range(45, 49):
                pixels[y][x] = (15, 45, 91)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "blue-voxel.png"
            write_png(path, pixels)
            metric = analyzer.measure(path)

        self.assertEqual(metric.blue_pixel_count, 12)
        self.assertEqual(metric.blue_bounds, (45, 40, 48, 42))
        self.assertEqual(metric.peak_rgb, (15, 45, 91))

    def test_colored_hud_outside_interior_roi_is_ignored(self) -> None:
        pixels = [[(0, 0, 0) for _ in range(100)] for _ in range(100)]
        pixels[10][90] = (0, 80, 200)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "hud.png"
            write_png(path, pixels)
            metric = analyzer.measure(path)

        self.assertEqual(metric.blue_pixel_count, 0)


if __name__ == "__main__":
    unittest.main()
