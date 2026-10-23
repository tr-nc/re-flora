"""Indexed diagnostic PNG support for the current butterfly authoring pipeline.

Preserved from the project's retired v2 runner; no old experiment is required.
The five-entry palette, alpha threshold and PNG encoding are unchanged.
"""

import struct
import zlib

from PIL import Image

BLUE = [(24, 32, 48), (63, 93, 124), (80, 190, 220), (205, 246, 248)]
GRAY = [(24, 24, 24), (80, 80, 80), (160, 160, 160), (240, 240, 240)]


def indexed(image, palette):
    result = Image.new("P", image.size, 0)
    result.putpalette([0, 0, 0] + [c for rgb in palette for c in rgb])
    indices = []
    for r, g, b, a in image.convert("RGBA").getdata():
        index = (
            0
            if a < 128
            else 1
            + min(
                range(4),
                key=lambda i: sum((v - p) ** 2 for v, p in zip((r, g, b), palette[i])),
            )
        )
        indices.append(index)
    result.putdata(indices)
    result.info["transparency"] = 0
    return result


def save_indexed(image, path):
    image.save(path, transparency=0, bits=4)
    data = path.read_bytes()
    out = [data[:8]]
    offset = 8
    while offset < len(data):
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        kind = data[offset + 4 : offset + 8]
        chunk = data[offset + 8 : offset + 8 + length]
        if kind == b"PLTE":
            chunk = chunk[:15]
        out.append(
            struct.pack(">I", len(chunk))
            + kind
            + chunk
            + struct.pack(">I", zlib.crc32(kind + chunk))
        )
        offset += length + 12
    path.write_bytes(b"".join(out))
