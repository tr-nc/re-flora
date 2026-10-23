#!/usr/bin/env python3
"""Rebuild mono leaf-loop derivatives without modifying the supplied stereo originals."""
from __future__ import annotations

import argparse
import array
import hashlib
import io
import json
import math
import sys
import wave
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GROUPS = ("leaves-breeze", "leaves-strong-wind")
SAMPLE_RATE = 48_000
CROSSFADE_FRAMES = 4_800
TARGET_RMS_DB = -30.0


def prepare(samples: list[float], overlap: int) -> list[float]:
    """Overlap tail onto head, retaining adjacent source samples at the loop boundary."""
    if overlap < 2 or len(samples) <= 2 * overlap:
        raise ValueError("loop must exceed twice a crossfade of at least two frames")
    mean = sum(samples) / len(samples)
    centered = [sample - mean for sample in samples]
    result = centered[:-overlap]
    for i in range(overlap):
        t = i / (overlap - 1)
        result[i] = centered[len(result) + i] * math.sqrt(1 - t) + centered[i] * math.sqrt(t)
    rms = math.sqrt(sum(sample * sample for sample in result) / len(result))
    if rms < 1e-9:
        raise ValueError("silent input cannot be level matched")
    gain = 10 ** (TARGET_RMS_DB / 20) / rms
    result = [sample * gain for sample in result]
    if max(map(abs, result)) >= 0.95:
        raise ValueError("level matching would exhaust PCM headroom; review the source")
    return result


def derivative(source: Path) -> bytes:
    with wave.open(str(source), "rb") as wav:
        if (wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getnframes()) != (
            2, 2, SAMPLE_RATE, 384_000
        ):
            raise ValueError(f"unexpected original format: {source}")
        pcm = array.array("h", wav.readframes(wav.getnframes()))
    if sys.byteorder != "little":
        pcm.byteswap()
    mono = [(pcm[i] + pcm[i + 1]) / 65536 for i in range(0, len(pcm), 2)]
    prepared = prepare(mono, CROSSFADE_FRAMES)
    output = array.array("h", (round(sample * 32768) for sample in prepared))
    if sys.byteorder != "little":
        output.byteswap()
    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(SAMPLE_RATE)
        wav.writeframes(output.tobytes())
    return buffer.getvalue()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify derivatives without writing")
    args = parser.parse_args()
    mismatches = []
    for group in GROUPS:
        directory = ROOT / "assets/sfx" / group
        records = []
        outputs: dict[Path, bytes] = {}
        for index in range(1, 5):
            name = f"{group}-{index:02}.wav"
            source = directory / name
            data = derivative(source)
            outputs[directory / "runtime" / name] = data
            records.append({
                "source": name,
                "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
                "runtime": f"runtime/{name}",
                "runtime_sha256": hashlib.sha256(data).hexdigest(),
            })
        manifest = {
            "generator": "scripts/prepare_tree_leaf_loops.py",
            "processing": "stereo mean to mono, DC removal, tail/head equal-power overlap, RMS match",
            "overlap_frames": CROSSFADE_FRAMES,
            "sample_rate": SAMPLE_RATE,
            "runtime_frames": 384_000 - CROSSFADE_FRAMES,
            "target_rms_dbfs": TARGET_RMS_DB,
            "rights": "Inherited Free-plan restrictions; no new license or commercial clearance",
            "files": records,
        }
        outputs[directory / "runtime/manifest.json"] = (json.dumps(manifest, indent=2) + "\n").encode()
        for path, data in outputs.items():
            if args.check:
                if not path.exists() or path.read_bytes() != data:
                    mismatches.append(str(path.relative_to(ROOT)))
            else:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(data)
        print(f"{group}: 4 mono loops, 379200 frames / 7.9 s, RMS {TARGET_RMS_DB:g} dBFS")
    for path in mismatches:
        print(f"stale or missing: {path}", file=sys.stderr)
    return bool(mismatches)


if __name__ == "__main__":
    raise SystemExit(main())
