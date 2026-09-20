#!/usr/bin/env python3
"""Reproduce diagnostic measurements and nearest-neighbor viewing derivatives.

No background removal, registration correction, repainting, palette cleanup or
production asset write is performed. Only actual generated PNGs are sampled.
Requires Python 3 standard library and ImageMagick 7 (`magick`).
"""
from pathlib import Path
import hashlib
import json
import struct
import subprocess

ROOT = Path(__file__).resolve().parent


def magick(*args):
    return subprocess.check_output(["magick", *map(str, args)])


def stats(path):
    data = path.read_bytes()
    width, height, depth, color_type = struct.unpack(">IIBB", data[16:26])
    rgb = magick(path, "-depth", "8", "rgb:-")
    alpha = magick(path, "-alpha", "extract", "-depth", "8", "gray:-")
    cells = []
    for row in range(5):
        for col in range(5):
            x0, x1 = round(col * width / 5), round((col + 1) * width / 5)
            y0, y1 = round(row * height / 5), round((row + 1) * height / 5)
            points = []
            for y in range(y0, y1):
                for x in range(x0, x1):
                    p = (y * width + x) * 3
                    vals = rgb[p:p + 3]
                    # Diagnostic foreground proxy only: dark outline/body or
                    # saturated wing. It excludes neutral pale baked backdrop.
                    if max(vals) < 125 or max(vals) - min(vals) >= 35:
                        points.append((x - x0, y - y0))
            cells.append({
                "row": row, "frame": col, "crop_xywh": [x0, y0, x1-x0, y1-y0],
                "diagnostic_foreground_pixel_count": len(points),
                "diagnostic_foreground_bbox": None if not points else [
                    min(p[0] for p in points), min(p[1] for p in points),
                    max(p[0] for p in points) + 1, max(p[1] for p in points) + 1],
                "diagnostic_foreground_centroid": None if not points else [
                    round(sum(p[0] for p in points)/len(points), 2),
                    round(sum(p[1] for p in points)/len(points), 2)],
            })
    return {
        "file": path.name, "sha256": hashlib.sha256(data).hexdigest(),
        "width": width, "height": height, "png_bit_depth": depth,
        "png_color_type": color_type,
        "color_type_name": {0:"grayscale",2:"RGB",3:"indexed",4:"gray+alpha",6:"RGBA"}[color_type],
        "unique_colors": int(magick("identify", "-format", "%k", path)),
        "alpha_values": sorted(set(alpha)),
        "transparent_pixels": alpha.count(0),
        "partially_transparent_pixels": sum(0 < a < 255 for a in alpha),
        "equal_integer_5x5_cells": width % 5 == 0 and height % 5 == 0,
        "diagnostic_cells": cells,
    }


result = {
    "route": "A - built-in image_gen, actual generated result",
    "status": "not production ready; failed alpha and animation invariants",
    "method": "one initial generation plus one targeted edit; no external paid service or API fallback",
    "selected_raw": "ai-route-a-candidate-v2.png",
    "requested": {
        "rows": 5, "columns": 5, "azimuth_degrees": [0,45,90,135,180],
        "phase_cycles": [0,.2,.4,.6,.8], "fps": 5, "loop_seconds": 1,
        "shared_wing_angles_degrees": [0,57,35,-35,-57],
        "production_sheet_pixels": [80,80], "production_cell_pixels": [16,16],
        "production_palette_entries": 5,
        "production_palette_note": "one transparent plus four opaque grayscale indices, recolored by runtime",
    },
    "processing": {
        "raw_retained": True,
        "derivative": "whole-sheet point/nearest-neighbor resize to 80x80; no recentering or alpha repair",
        "crop": "each original cell uses round(n*dimension/5) boundaries, no per-sprite fitted crop",
        "measurement_warning": "dark-or-saturated foreground proxy is not a segmentation asset and its centroid is not an anatomical pivot",
    },
    "results": [],
}
for version in (1, 2):
    path = ROOT / f"ai-route-a-candidate-v{version}.png"
    entry = stats(path)
    small = ROOT / f"ai-route-a-v{version}-nearest-80.png"
    magick(path, "-filter", "point", "-resize", "80x80!", small)
    entry["nearest_80_file"] = small.name
    entry["nearest_80_sha256"] = hashlib.sha256(small.read_bytes()).hexdigest()
    result["results"].append(entry)
result["acceptance"] = {
    "25_butterflies": "pass - visual 5x5 population present",
    "same_blue_purple_identity": "partial - recognizable family, markings and apparent scale drift",
    "true_alpha": "fail - both raw results are RGB with zero transparent pixels",
    "regular_integer_grid": "fail - raw 1254x1254 cannot divide equally into five integer cells",
    "registration": "fail - visible cell-relative drift; no anatomical pivot lock guaranteed",
    "same_column_phase_across_rows": "fail - side-view sequence largely narrows/shrinks instead of sharing a verifiable wing angle",
    "seamless_loop": "fail - narrow final pose jumps back to wide first pose; return stroke not convincing",
    "production_indexed_palette": "fail - true-color large images, not one transparent plus four opaque grayscale entries",
    "16px_art_quality": "fail - nearest reduction exposes lost antennae/wing structure and backdrop contamination",
    "runtime_replacement": "not attempted; neither raw nor derivative is a production replacement",
}
(ROOT / "ai-route-a-manifest.json").write_text(json.dumps(result, indent=2, ensure_ascii=False)+"\n")
print(json.dumps({"results": [{k:v for k,v in e.items() if k != "diagnostic_cells"} for e in result["results"]], "acceptance": result["acceptance"]}, indent=2))
