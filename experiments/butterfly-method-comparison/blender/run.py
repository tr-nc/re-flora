#!/usr/bin/env python3
"""One-command throwaway Blender experiment, including a saved-file replay gate.

/usr/bin/python3 run.py
Optional: BLENDER=/absolute/path/to/blender /usr/bin/python3 run.py
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import zlib

from PIL import Image, ImageChops

ROOT = Path(__file__).resolve().parent
BLENDER = os.environ.get("BLENDER") or shutil.which("blender") or str(Path.home() / ".local/opt/blender-4.5.13-linux-x64/blender")
SCRIPT = ROOT / "create_export_blender.py"
ANGLES = [0, 45, 90, 135, 180]


def run_blender(mode, destination, blend=None):
    command = [BLENDER, "--background", "--factory-startup"]
    if blend:
        command.append(str(blend))
    command += ["--python", str(SCRIPT), "--", "--mode", mode, "--output", str(destination)]
    print(f"Blender {mode}: {destination}", flush=True)
    with (ROOT / f"{mode}-{destination.name}.log").open("w") as log:
        subprocess.run(command, check=True, stdout=log, stderr=subprocess.STDOUT)


def trim_palette(path):
    # Pillow pads a 4-bit indexed PNG's PLTE to 16 entries. The game contract
    # requires exactly five entries. PNG permits fewer entries than bit depth.
    # Drop only unused PLTE padding, preserve pixel data and rebuild that CRC.
    blob = path.read_bytes()
    chunks, cursor = [blob[:8]], 8
    while cursor < len(blob):
        length = struct.unpack(">I", blob[cursor:cursor + 4])[0]
        kind = blob[cursor + 4:cursor + 8]
        data = blob[cursor + 8:cursor + 8 + length]
        if kind == b"PLTE":
            data = data[:15]
        chunks.append(struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data)))
        cursor += 12 + length
    path.write_bytes(b"".join(chunks))


def indexed(source, palette):
    # Palette index 0 is transparent. A fixed global palette is reused for all
    # frames; no per-frame quantization, pivot shift, crop-to-bounds or dithering.
    rgba = source.convert("RGBA")
    output = Image.new("P", source.size, 0)
    output.putpalette([0, 0, 0] + [c for rgb in palette for c in rgb])
    indices = []
    for r, g, b, a in rgba.getdata():
        if a < 128:
            indices.append(0)
        else:
            distances = [sum((v - p) ** 2 for v, p in zip((r, g, b), rgb)) for rgb in palette]
            indices.append(1 + min(range(4), key=distances.__getitem__))
    output.putdata(indices)
    output.info["transparency"] = 0
    return output


def assemble():
    glb_bytes = (ROOT / "butterfly-prototype.glb").read_bytes()
    glb_json_length = struct.unpack("<I", glb_bytes[12:16])[0]
    glb = json.loads(glb_bytes[20:20 + glb_json_length])
    animations = glb["animations"]
    assert len(animations) == 1 and len(animations[0]["channels"]) == 2, "GLB must keep both wing hinges in one shared clip"
    time_accessor = glb["accessors"][animations[0]["samplers"][0]["input"]]
    assert time_accessor["min"] == [0] and time_accessor["max"] == [1], "GLB source timing must be exactly 0 through 1 second"
    raw = Image.new("RGBA", (640, 640))
    records = []
    replay_matches = []
    for row, azimuth in enumerate(ANGLES):
        for col in range(5):
            filename = f"view{azimuth:03d}_frame{col:02d}.png"
            source = Image.open(ROOT / "raw" / filename).convert("RGBA")
            replay = Image.open(ROOT / "replay" / filename).convert("RGBA")
            replay_matches.append(source.tobytes() == replay.tobytes())
            raw.paste(source, (col * 128, row * 128))
            alpha = source.getchannel("A")
            records.append({"row": row, "column": col, "azimuth": azimuth, "raw_bbox": alpha.getbbox(),
                "raw_boundary_alpha": max(max(alpha.crop(box).getdata()) for box in [(0, 0, 128, 1), (0, 127, 128, 128), (0, 0, 1, 128), (127, 0, 128, 128)])})
    raw.save(ROOT / "atlas-128-rgba.png")
    palettes = {
        "blue": [(20, 27, 40), (48, 108, 165), (74, 194, 228), (190, 246, 250)],
        "gray": [(24, 24, 24), (80, 80, 80), (160, 160, 160), (240, 240, 240)],
    }
    outputs = {}
    for size in (16, 32):
        resized = raw.resize((size * 5, size * 5), Image.Resampling.BOX)
        resized.save(ROOT / f"atlas-{size}-rgba.png")
        for name, palette in palettes.items():
            working = resized
            if name == "gray":
                working = Image.merge("RGBA", (*[resized.convert("L")] * 3, resized.getchannel("A")))
            result = indexed(working, palette)
            filename = f"atlas-{size}-{name}-indexed.png"
            result.save(ROOT / filename, transparency=0, optimize=False, bits=4)
            trim_palette(ROOT / filename)
            reopened = Image.open(ROOT / filename)
            rgba = reopened.convert("RGBA")
            alpha_values = sorted(set(rgba.getchannel("A").getdata()))
            blob = (ROOT / filename).read_bytes()
            cursor = 8
            plte_entries = None
            while cursor < len(blob):
                length = struct.unpack(">I", blob[cursor:cursor + 4])[0]
                if blob[cursor + 4:cursor + 8] == b"PLTE":
                    plte_entries = length // 3
                cursor += 12 + length
            outputs[filename] = {"size": list(reopened.size), "mode": reopened.mode,
                "palette_entries": plte_entries, "used_indices": sorted(set(reopened.getdata())), "alpha_values": alpha_values,
                "sha256": hashlib.sha256(blob).hexdigest()}
    final = Image.open(ROOT / "atlas-16-blue-indexed.png").convert("RGBA")
    phase_metrics = []
    for row, azimuth in enumerate(ANGLES):
        frames = [final.crop((col * 16, row * 16, (col + 1) * 16, (row + 1) * 16)) for col in range(5)]
        phase_metrics.append({"azimuth": azimuth, "unique_frames": len({im.tobytes() for im in frames}),
            "changed_pixels_to_next_frame_including_wrap": [sum(a != b for a, b in zip(frames[i].getdata(), frames[(i + 1) % 5].getdata())) for i in range(5)],
            "opaque_pixels_per_frame": [sum(a == 255 for a in im.getchannel("A").getdata()) for im in frames]})
    endpoint = Image.open(ROOT / "raw/loop_endpoint_view045.png").convert("RGBA")
    start = Image.open(ROOT / "raw/view045_frame00.png").convert("RGBA")
    endpoint_match = endpoint.tobytes() == start.tobytes()
    result = {
        "glb_shared_clip": {"animation_count": len(animations), "channels": len(animations[0]["channels"]), "targets": [c["target"] for c in animations[0]["channels"]]},
        "saved_blend_replay": {"frames_compared": 25, "all_rgba_pixels_identical": all(replay_matches)},
        "true_loop_endpoint": {"t0_equals_t1_rgba": endpoint_match},
        "all_raw_frames_have_clear_boundary": all(r["raw_boundary_alpha"] == 0 for r in records),
        "resampling": "Whole atlas BOX 128px to 16px/32px cells; no per-frame crops or recentering. Alpha < 128 transparent, >= 128 opaque; nearest fixed palette, no dithering.",
        "outputs": outputs, "raw_frames": records,
        "sampled_16px_phase_metrics": phase_metrics,
        "art_verdict": "Structural prototype only. Shared phase/geometry are guaranteed by source; five-frame stepping and small edge aliasing remain. No hand-cleaned pixel art approval is implied.",
    }
    (ROOT / "validation.json").write_text(json.dumps(result, indent=2) + "\n")
    assert all(replay_matches), "Saved Blender project failed exact pixel replay"
    assert endpoint_match, "True t=0/t=1 loop endpoint mismatch"
    assert result["all_raw_frames_have_clear_boundary"], "Source silhouette hits frame boundary"
    assert all(o["palette_entries"] == 5 and o["alpha_values"] == [0, 255] for o in outputs.values())
    print("PASS: 25/25 saved-project replays exact; loop endpoint exact; no clipping; four PNGs use exactly five palette entries and binary alpha.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--export-existing", action="store_true", help="Preserve and export the editable .blend instead of recreating it from the recipe")
    args = parser.parse_args()
    if not args.export_existing:
        run_blender("create", ROOT)
    # Always export the interactive model by reopening the saved project too.
    run_blender("export-glb", ROOT, ROOT / "butterfly-prototype.blend")
    run_blender("render", ROOT / "raw", ROOT / "butterfly-prototype.blend")
    run_blender("render", ROOT / "replay", ROOT / "butterfly-prototype.blend")
    run_blender("preview", ROOT / "turntable-frames", ROOT / "butterfly-prototype.blend")
    assemble()
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-framerate", "15", "-i", str(ROOT / "turntable-frames/turntable_%03d.png"),
        "-vf", "format=yuv420p", "-c:v", "libvpx-vp9", "-crf", "24", "-b:v", "0", str(ROOT / "blender-source-turntable.webm")], check=True)
