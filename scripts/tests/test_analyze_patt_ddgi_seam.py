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

import analyze_patt_ddgi_seam as analyzer  # noqa: E402


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
        analyzer.PNG_SIGNATURE
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def diagonal_image(*, banded: bool) -> list[list[int]]:
    width = 960
    height = 540
    pixels: list[list[int]] = []
    for y in range(height):
        row = []
        for x in range(width):
            coordinate = y - 0.5 * x
            edge = 25.0
            if not banded:
                value = 40 if coordinate < edge else 180
            elif coordinate < edge - 20:
                value = 40
            elif coordinate < edge:
                value = 80
            elif coordinate < edge + 20:
                value = 150
            else:
                value = 180
            row.append(value)
        pixels.append(row)
    return pixels


class PattDdgiSeamAnalyzerTests(unittest.TestCase):
    def test_single_hard_projected_edge_is_green(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "hard-edge.png"
            write_png(path, diagonal_image(banded=False))

            metric = analyzer.measure(path)

        self.assertFalse(metric.is_red)
        self.assertEqual(metric.secondary_band_count, 0)

    def test_secondary_spatial_bands_are_red_after_primary_edge_exclusion(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "banded.png"
            write_png(path, diagonal_image(banded=True))

            metric = analyzer.measure(path)

        self.assertTrue(metric.is_red)
        self.assertGreaterEqual(metric.secondary_band_count, 2)
        self.assertGreaterEqual(
            metric.secondary_primary_ratio, analyzer.MIN_SECONDARY_PRIMARY_RATIO
        )


if __name__ == "__main__":
    unittest.main()
