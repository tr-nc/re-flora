"""Display-RGB temporal diagnostics. No numpy/Pillow dependency; ImageMagick decodes PNGs.

ROIs are normalized rectangles. Statistics use absolute per-pixel mean-RGB differences
in code values, not luminance or HDR radiance. Never interpret a real geometry/light
change as noise without a separate stable-surface control.
"""
import math
import re
import subprocess
from pathlib import Path


# Fixed camera receiver rectangles. Do not move these to select favorable A/B results.
WALL_ROIS = {"wall_left": (.25, .60, .45, .80),
             "wall_right": (.55, .60, .75, .80)}


def percentile(values, fraction):
    if not values:
        raise ValueError("percentile requires samples")
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * fraction) - 1)]


def summary(values):
    return {"p95": percentile(values, .95), "p99": percentile(values, .99),
            "max": max(values), "mean": sum(values) / len(values)}


def frame_delta(previous, current, spike_threshold):
    if len(previous) != len(current) or not current or len(current) % 3:
        raise ValueError("RGB frames must have equal nonzero dimensions")
    delta = [sum(abs(current[i + c] - previous[i + c]) for c in range(3)) / 3
             for i in range(0, len(current), 3)]
    return {**summary(delta),
            "rgb_rmse": math.sqrt(sum((a - b) ** 2 for a, b in zip(previous, current)) / len(current)),
            "spike_area": sum(v > spike_threshold for v in delta) / len(delta)}


def read_roi(path, roi):
    width, height = map(int, subprocess.check_output(
        ["magick", str(path), "-format", "%w %h", "info:"], text=True).split())
    if abs(width / height - 16 / 9) > .01:
        raise ValueError("fixture ROI requires 16:9")
    x0, y0, x1, y1 = roi
    if not (0 <= x0 < x1 <= 1 and 0 <= y0 < y1 <= 1):
        raise ValueError("ROI must be a nonempty normalized rectangle")
    crop = f"{int(width*x1)-int(width*x0)}x{int(height*y1)-int(height*y0)}+{int(width*x0)}+{int(height*y0)}"
    return subprocess.check_output(["magick", str(path), "-crop", crop,
                                    "+repage", "-alpha", "off", "-depth", "8", "rgb:-"])


def analyze(console, prefix, rois, spike_threshold=3):
    """Select captures at recording time between real publication begin/end markers.

    Require all claimed frames to exist and have a successful write marker. A caller
    must use an empty output directory; no stale screenshot can satisfy this contract.
    """
    begin, end = console.find("[DDGI_SUSTAINED] begin"), console.find("[DDGI_SUSTAINED] end")
    if not 0 <= begin < end:
        raise ValueError("missing complete sustained-edit interval")
    captures = []
    for match in re.finditer(r"\[SCREENSHOT\] Capturing after ([0-9.]+)s to (.+)", console):
        path = Path(match[2].strip())
        if not str(path).startswith(str(prefix) + "."):
            continue
        if not path.is_file() or not re.search(r"\[SCREENSHOT\] Saved \d+x\d+ to " + re.escape(str(path)) + r"(?:\n|$)", console):
            raise ValueError(f"missing completed fresh capture: {path}")
        if begin < match.start() < end:
            captures.append((float(match[1]), path))
    if len(captures) < 3:
        raise ValueError("fewer than three captures during sustained edits")
    times = [time for time, _ in captures]
    gaps = [b - a for a, b in zip(times, times[1:])]
    if min(gaps) <= 0:
        raise ValueError("nonmonotonic capture times")
    report = {"captures": len(captures), "capture_gap_seconds": summary(gaps),
              "files": [str(path) for _, path in captures], "rois": {},
              "spike_threshold_rgb_u8": spike_threshold}
    for name, roi in rois.items():
        frames = []
        previous = None
        for time, path in captures:
            current = read_roi(path, roi)
            if previous is not None:
                frames.append({"time_seconds": time, **frame_delta(previous, current, spike_threshold)})
            previous = current
        report["rois"][name] = {"rectangle": roi, "frames": frames,
            "temporal": {key: summary([frame[key] for frame in frames])
                         for key in ("mean", "p95", "p99", "max", "spike_area")}}
    return report
