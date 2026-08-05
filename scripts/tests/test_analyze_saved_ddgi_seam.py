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

import analyze_saved_ddgi_seam as analyzer  # noqa: E402
import analyze_patt_ddgi_seam as png  # noqa: E402


def write_png(path: Path, pixels: list[list[int]]) -> None:
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
        for value in row:
            raw.extend((value, value, value, 255))
    path.write_bytes(
        png.PNG_SIGNATURE
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def horizontal_image(*, banded: bool) -> list[list[int]]:
    width = 400
    height = 300
    pixels: list[list[int]] = []
    for y in range(height):
        if not banded:
            value = 180 if y >= 220 else 40
        elif y < 80:
            value = 40
        elif y < 120:
            value = 80
        elif y < 160:
            value = 120
        elif y < 200:
            value = 150
        else:
            value = 180
        pixels.append([value] * width)
    return pixels


class SavedDdgiSeamAnalyzerTests(unittest.TestCase):
    def test_single_hard_edge_is_green(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "hard-edge.png"
            write_png(path, horizontal_image(banded=False))
            metric = analyzer.measure(path)

        self.assertFalse(metric.is_red)
        self.assertEqual(metric.internal_band_count, 0)

    def test_internal_bands_are_red_after_edge_exclusion(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "banded.png"
            write_png(path, horizontal_image(banded=True))
            metric = analyzer.measure(path)

        self.assertTrue(metric.is_red)
        self.assertGreaterEqual(metric.internal_band_count, 2)
        self.assertGreaterEqual(
            metric.internal_primary_ratio, analyzer.MIN_INTERNAL_PRIMARY_RATIO
        )


if __name__ == "__main__":
    unittest.main()
