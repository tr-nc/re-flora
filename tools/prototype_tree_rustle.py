#!/usr/bin/env python3
"""Fast procedural tree-rustle sound prototype.

This is intentionally dependency-free so it can be run with uv immediately:

    cd tools
    uv run python prototype_tree_rustle.py
    uv run python prototype_tree_rustle.py --preset dry --seed 7 --wind 0.8
    uv run python prototype_tree_rustle.py --preset storm --duration 6 --no-play

The synthesis model is event-based instead of loop-based:

- slow gust envelope controls all layers
- filtered broadband noise forms the airy wind bed
- band-passed noise forms constant leaf friction
- many short randomized grains form leaf flutter/rattle clusters
- sparse low creaks add branch motion during stronger wind
"""

from __future__ import annotations

import argparse
import math
import random
import shutil
import struct
import subprocess
import sys
import wave
from array import array
from dataclasses import dataclass, replace
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / "target" / "audio-prototypes" / "tree_rustle.wav"
TWO_PI = math.tau


@dataclass(frozen=True)
class RustlePreset:
    base_wind: float
    gustiness: float
    leaf_density: float
    dryness: float
    branch: float
    air: float
    description: str


PRESETS = {
    "soft": RustlePreset(
        base_wind=0.35,
        gustiness=0.35,
        leaf_density=0.70,
        dryness=0.30,
        branch=0.02,
        air=0.45,
        description="gentle broadleaf canopy; soft and non-invasive",
    ),
    "dry": RustlePreset(
        base_wind=0.55,
        gustiness=0.55,
        leaf_density=1.05,
        dryness=0.92,
        branch=0.05,
        air=0.35,
        description="papery dry leaves with bright crackly transients",
    ),
    "dense": RustlePreset(
        base_wind=0.62,
        gustiness=0.50,
        leaf_density=1.35,
        dryness=0.55,
        branch=0.08,
        air=0.65,
        description="wide dense canopy with many overlapping leaf clusters",
    ),
    "storm": RustlePreset(
        base_wind=0.88,
        gustiness=0.95,
        leaf_density=1.85,
        dryness=0.68,
        branch=0.22,
        air=1.0,
        description="heavy gusts, fast flutter, and occasional branch stress",
    ),
}


@dataclass(slots=True)
class Grain:
    delay: int
    env: float
    target: float
    attack_alpha: float
    decay: float
    hp_state: float
    hp_alpha: float
    lp_state: float
    lp_alpha: float
    pan_l: float
    pan_r: float


@dataclass(slots=True)
class BranchCreak:
    env: float
    decay: float
    phase: float
    wobble_phase: float
    frequency: float
    wobble_frequency: float
    amp: float
    pan_l: float
    pan_r: float
    noise_lp: float


def clamp(value: float, low: float, high: float) -> float:
    return min(high, max(low, value))


def lowpass_alpha(cutoff_hz: float, sample_rate: int) -> float:
    cutoff_hz = clamp(cutoff_hz, 1.0, sample_rate * 0.45)
    return 1.0 - math.exp(-TWO_PI * cutoff_hz / sample_rate)


def equal_power_pan(pan: float) -> tuple[float, float]:
    pan = clamp(pan, -1.0, 1.0)
    angle = (pan + 1.0) * math.pi * 0.25
    return math.cos(angle), math.sin(angle)


def build_wind_envelope(
    *,
    duration_seconds: float,
    control_rate: int,
    base_wind: float,
    gustiness: float,
    rng: random.Random,
) -> list[float]:
    """Build a normalized 0..1 gust curve at control rate."""
    frame_count = int(math.ceil(duration_seconds * control_rate)) + 2
    base_wind = clamp(base_wind, 0.0, 1.0)
    gustiness = clamp(gustiness, 0.0, 1.0)

    values = [base_wind for _ in range(frame_count)]

    # Slow random wander keeps the bed from feeling looped or static.
    target = base_wind
    wander = base_wind
    for index in range(frame_count):
        target += rng.uniform(-0.028, 0.028) * gustiness
        target += (base_wind - target) * 0.015
        target = clamp(target, base_wind * 0.45, min(1.0, base_wind + 0.22 + 0.25 * gustiness))
        wander += (target - wander) * 0.045
        values[index] = 0.72 * values[index] + 0.28 * wander

    # Raised-cosine gusts create the important swell/peak/release motion.
    gust_rate_per_second = 0.05 + 0.23 * gustiness
    had_gust = False
    for index in range(frame_count):
        if rng.random() >= gust_rate_per_second / control_rate:
            continue

        had_gust = True
        gust_duration = rng.uniform(0.65, 2.8 + 1.4 * gustiness)
        gust_frames = max(4, int(gust_duration * control_rate))
        gust_amp = rng.uniform(0.12, 0.50) * gustiness
        for offset in range(gust_frames):
            out_index = index + offset
            if out_index >= frame_count:
                break
            t = offset / max(1, gust_frames - 1)
            shape = math.sin(math.pi * t) ** 1.35
            # Add a small shoulder so the gust does not sound like a pure LFO.
            shoulder = math.sin(math.pi * min(1.0, t * 1.8)) ** 2 * (1.0 - t) * 0.22
            values[out_index] += gust_amp * (shape + shoulder)

    # Short previews can miss random gusts; force one modest swell for auditioning.
    if not had_gust and gustiness > 0.20 and frame_count > control_rate:
        start = int(frame_count * 0.18)
        gust_frames = min(frame_count - start, int((0.9 + 1.2 * gustiness) * control_rate))
        for offset in range(gust_frames):
            t = offset / max(1, gust_frames - 1)
            values[start + offset] += 0.24 * gustiness * (math.sin(math.pi * t) ** 1.35)

    # Final inertial smoothing: quicker attack, slower release.
    smoothed = []
    current = values[0]
    for value in values:
        value = clamp(value, 0.0, 1.0)
        alpha = 0.12 if value >= current else 0.042
        current += (value - current) * alpha
        smoothed.append(clamp(current, 0.0, 1.0))
    return smoothed


def make_grain(
    *,
    rng: random.Random,
    sample_rate: int,
    wind: float,
    dryness: float,
    delay: int,
) -> Grain:
    duration = rng.uniform(0.010, 0.075 - 0.020 * dryness)
    duration = max(0.006, duration)
    decay = math.exp(-1.0 / (duration * sample_rate))
    attack_ms = rng.uniform(1.0, 5.0)
    attack_alpha = 1.0 - math.exp(-1.0 / (attack_ms * 0.001 * sample_rate))

    hp_cutoff = rng.uniform(900.0 + 900.0 * dryness, 3200.0 + 2600.0 * dryness)
    lp_cutoff = rng.uniform(5500.0 + 3500.0 * dryness, 10500.0 + 6500.0 * dryness)
    amp = rng.uniform(0.015, 0.075) * (0.45 + wind) * (0.75 + 0.55 * dryness)
    pan_l, pan_r = equal_power_pan(rng.uniform(-0.92, 0.92))

    return Grain(
        delay=delay,
        env=0.0,
        target=amp,
        attack_alpha=attack_alpha,
        decay=decay,
        hp_state=0.0,
        hp_alpha=lowpass_alpha(hp_cutoff, sample_rate),
        lp_state=0.0,
        lp_alpha=lowpass_alpha(lp_cutoff, sample_rate),
        pan_l=pan_l,
        pan_r=pan_r,
    )


def make_branch_creak(
    *,
    rng: random.Random,
    sample_rate: int,
    wind: float,
    branch: float,
) -> BranchCreak:
    duration = rng.uniform(0.35, 1.25)
    pan_l, pan_r = equal_power_pan(rng.uniform(-0.65, 0.65))
    return BranchCreak(
        env=rng.uniform(0.035, 0.090) * wind * branch,
        decay=math.exp(-1.0 / (duration * sample_rate)),
        phase=rng.uniform(0.0, TWO_PI),
        wobble_phase=rng.uniform(0.0, TWO_PI),
        frequency=rng.uniform(75.0, 210.0),
        wobble_frequency=rng.uniform(1.1, 4.7),
        amp=1.0,
        pan_l=pan_l,
        pan_r=pan_r,
        noise_lp=0.0,
    )


def render_rustle(
    *,
    preset: RustlePreset,
    duration_seconds: float,
    sample_rate: int,
    seed: int,
) -> tuple[array, array, list[float]]:
    rng = random.Random(seed)
    control_rate = 100
    controls = build_wind_envelope(
        duration_seconds=duration_seconds,
        control_rate=control_rate,
        base_wind=preset.base_wind,
        gustiness=preset.gustiness,
        rng=rng,
    )
    sample_count = int(round(duration_seconds * sample_rate))
    samples_per_control = sample_rate / control_rate

    left = array("f")
    right = array("f")
    grains: list[Grain] = []
    creaks: list[BranchCreak] = []

    air_lp_l = air_lp_r = 0.0
    air_slow_l = air_slow_r = 0.0
    leaf_hp_l = leaf_hp_r = 0.0
    leaf_lp_l = leaf_lp_r = 0.0

    for index in range(sample_count):
        control_index = min(len(controls) - 1, int(index / samples_per_control))
        wind = controls[control_index]
        wind_lift = wind ** 1.35

        # Wide airy wind bed: low/mid filtered noise, brighter under gusts.
        air_cutoff = 700.0 + 1450.0 * wind
        air_alpha = lowpass_alpha(air_cutoff, sample_rate)
        air_slow_alpha = lowpass_alpha(90.0 + 70.0 * wind, sample_rate)
        raw_l = rng.uniform(-1.0, 1.0)
        raw_r = rng.uniform(-1.0, 1.0)
        air_lp_l += (raw_l - air_lp_l) * air_alpha
        air_lp_r += (raw_r - air_lp_r) * air_alpha
        air_slow_l += (air_lp_l - air_slow_l) * air_slow_alpha
        air_slow_r += (air_lp_r - air_slow_r) * air_slow_alpha
        air_amp = 0.115 * preset.air * (0.16 + wind_lift)
        out_l = (air_lp_l - 0.78 * air_slow_l) * air_amp
        out_r = (air_lp_r - 0.78 * air_slow_r) * air_amp

        # Continuous leaf friction bed: high-passed noise that brightens with wind/dryness.
        leaf_hp_cutoff = 850.0 + 1200.0 * preset.dryness + 900.0 * wind
        leaf_lp_cutoff = 5200.0 + 7200.0 * preset.dryness + 4200.0 * wind
        leaf_hp_alpha = lowpass_alpha(leaf_hp_cutoff, sample_rate)
        leaf_lp_alpha = lowpass_alpha(leaf_lp_cutoff, sample_rate)
        raw_l = rng.uniform(-1.0, 1.0)
        raw_r = rng.uniform(-1.0, 1.0)
        leaf_hp_l += (raw_l - leaf_hp_l) * leaf_hp_alpha
        leaf_hp_r += (raw_r - leaf_hp_r) * leaf_hp_alpha
        high_l = raw_l - leaf_hp_l
        high_r = raw_r - leaf_hp_r
        leaf_lp_l += (high_l - leaf_lp_l) * leaf_lp_alpha
        leaf_lp_r += (high_r - leaf_lp_r) * leaf_lp_alpha
        leaf_amp = 0.050 * preset.leaf_density * (wind ** 1.8) * (0.70 + 0.70 * preset.dryness)
        out_l += leaf_lp_l * leaf_amp
        out_r += leaf_lp_r * leaf_amp

        # Leaf bursts: clustered short grains, with density tied to gust strength.
        burst_rate = (0.8 + 30.0 * (wind ** 2.25)) * preset.leaf_density
        if rng.random() < burst_rate / sample_rate:
            cluster_count = 1 + rng.randrange(1 + int(2 + 6 * wind))
            if rng.random() < 0.20 + 0.35 * wind:
                cluster_count += rng.randrange(2, 7)
            cluster_window = int(sample_rate * rng.uniform(0.006, 0.055 + 0.035 * wind))
            for _ in range(cluster_count):
                delay = rng.randrange(max(1, cluster_window))
                grains.append(
                    make_grain(
                        rng=rng,
                        sample_rate=sample_rate,
                        wind=wind,
                        dryness=preset.dryness,
                        delay=delay,
                    )
                )

        # Sparse branch stress: low, subtle, and only likely under stronger wind.
        branch_rate = preset.branch * max(0.0, wind - 0.42) ** 2 * 1.15
        if rng.random() < branch_rate / sample_rate:
            creaks.append(
                make_branch_creak(
                    rng=rng,
                    sample_rate=sample_rate,
                    wind=wind,
                    branch=preset.branch,
                )
            )

        next_grains: list[Grain] = []
        for grain in grains:
            if grain.delay > 0:
                grain.delay -= 1
                next_grains.append(grain)
                continue

            grain.target *= grain.decay
            grain.env += (grain.target - grain.env) * grain.attack_alpha
            raw = rng.uniform(-1.0, 1.0)
            grain.hp_state += (raw - grain.hp_state) * grain.hp_alpha
            high = raw - grain.hp_state
            grain.lp_state += (high - grain.lp_state) * grain.lp_alpha
            sample = grain.lp_state * grain.env
            out_l += sample * grain.pan_l
            out_r += sample * grain.pan_r
            if grain.env > 0.00005 or grain.target > 0.00005:
                next_grains.append(grain)
        grains = next_grains

        next_creaks: list[BranchCreak] = []
        for creak in creaks:
            creak.env *= creak.decay
            creak.wobble_phase += TWO_PI * creak.wobble_frequency / sample_rate
            wobble = 1.0 + 0.11 * math.sin(creak.wobble_phase) + 0.035 * rng.uniform(-1.0, 1.0)
            creak.phase += TWO_PI * creak.frequency * wobble / sample_rate
            creak.noise_lp += (rng.uniform(-1.0, 1.0) - creak.noise_lp) * 0.018
            tone = math.sin(creak.phase) + 0.35 * math.sin(creak.phase * 2.03 + 0.7)
            sample = (0.72 * tone + 0.28 * creak.noise_lp) * creak.env * creak.amp
            out_l += sample * creak.pan_l
            out_r += sample * creak.pan_r
            if creak.env > 0.00003:
                next_creaks.append(creak)
        creaks = next_creaks

        left.append(out_l)
        right.append(out_r)

    return left, right, controls


def write_wav(
    path: Path,
    *,
    left: array,
    right: array,
    sample_rate: int,
    normalize: bool,
    peak_target: float,
) -> tuple[float, float]:
    path.parent.mkdir(parents=True, exist_ok=True)
    peak = 0.0
    for l_sample, r_sample in zip(left, right):
        peak = max(peak, abs(l_sample), abs(r_sample))

    if normalize and peak > 0.0:
        scale = peak_target / peak
    else:
        scale = 1.0

    fade_samples = min(len(left) // 3, int(0.035 * sample_rate))
    frames = bytearray()
    for index, (l_sample, r_sample) in enumerate(zip(left, right)):
        fade = 1.0
        if fade_samples > 0:
            fade = min(1.0, index / fade_samples, (len(left) - index - 1) / fade_samples)
        l_value = clamp(l_sample * scale * fade, -1.0, 1.0)
        r_value = clamp(r_sample * scale * fade, -1.0, 1.0)
        frames += struct.pack("<hh", int(l_value * 32767.0), int(r_value * 32767.0))

    with wave.open(str(path), "wb") as writer:
        writer.setnchannels(2)
        writer.setsampwidth(2)
        writer.setframerate(sample_rate)
        writer.writeframes(frames)

    return peak, scale


def play_wav(path: Path) -> bool:
    commands: list[list[str]] = []
    if sys.platform == "darwin":
        commands.append(["afplay", str(path)])
    elif sys.platform.startswith("linux"):
        commands.extend(
            [
                ["paplay", str(path)],
                ["aplay", str(path)],
                ["ffplay", "-nodisp", "-autoexit", "-loglevel", "quiet", str(path)],
            ]
        )
    elif sys.platform.startswith("win"):
        commands.append(
            [
                "powershell",
                "-NoProfile",
                "-Command",
                f"(New-Object Media.SoundPlayer '{path}').PlaySync();",
            ]
        )

    for command in commands:
        if shutil.which(command[0]) is None:
            continue
        subprocess.run(command, check=False)
        return True
    return False


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Render and optionally play a procedural tree rustling WAV prototype.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--preset", choices=sorted(PRESETS), default="dense")
    parser.add_argument("--duration", type=float, default=8.0, help="seconds to render")
    parser.add_argument("--sample-rate", type=int, default=48_000)
    parser.add_argument("--seed", type=int, default=3)
    parser.add_argument("--out", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--wind", type=float, help="override normalized base wind, 0..1")
    parser.add_argument("--gustiness", type=float, help="override gust amount, 0..1")
    parser.add_argument("--leaf-density", type=float, help="override leaf burst density multiplier")
    parser.add_argument("--dryness", type=float, help="override brightness/crackle amount, 0..1")
    parser.add_argument("--branch", type=float, help="override branch-creak amount, 0..1")
    parser.add_argument("--air", type=float, help="override airy wind-bed amount, 0..1")
    parser.add_argument("--peak", type=float, default=0.92, help="normalized WAV peak when normalizing")
    parser.add_argument("--normalize", dest="normalize", action="store_true", default=True)
    parser.add_argument("--no-normalize", dest="normalize", action="store_false")
    parser.add_argument("--play", dest="play", action="store_true", default=True)
    parser.add_argument("--no-play", dest="play", action="store_false")
    parser.add_argument("--list-presets", action="store_true", help="print preset descriptions and exit")
    return parser


def apply_overrides(preset: RustlePreset, args: argparse.Namespace) -> RustlePreset:
    changes: dict[str, float] = {}
    if args.wind is not None:
        changes["base_wind"] = clamp(args.wind, 0.0, 1.0)
    if args.gustiness is not None:
        changes["gustiness"] = clamp(args.gustiness, 0.0, 1.0)
    if args.leaf_density is not None:
        changes["leaf_density"] = max(0.0, args.leaf_density)
    if args.dryness is not None:
        changes["dryness"] = clamp(args.dryness, 0.0, 1.0)
    if args.branch is not None:
        changes["branch"] = clamp(args.branch, 0.0, 1.0)
    if args.air is not None:
        changes["air"] = clamp(args.air, 0.0, 1.0)
    return replace(preset, **changes)


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()

    if args.list_presets:
        for name, preset in sorted(PRESETS.items()):
            print(f"{name}: {preset.description}")
        return 0

    if args.duration <= 0.0:
        parser.error("--duration must be positive")
    if args.sample_rate < 8_000:
        parser.error("--sample-rate must be at least 8000")
    if not 0.0 < args.peak <= 1.0:
        parser.error("--peak must be in (0, 1]")

    preset = apply_overrides(PRESETS[args.preset], args)
    print(
        "rendering "
        f"preset={args.preset} seed={args.seed} duration={args.duration:.2f}s "
        f"wind={preset.base_wind:.2f} gustiness={preset.gustiness:.2f} "
        f"leaf_density={preset.leaf_density:.2f} dryness={preset.dryness:.2f} "
        f"branch={preset.branch:.2f} air={preset.air:.2f}"
    )

    left, right, controls = render_rustle(
        preset=preset,
        duration_seconds=args.duration,
        sample_rate=args.sample_rate,
        seed=args.seed,
    )
    peak, scale = write_wav(
        args.out,
        left=left,
        right=right,
        sample_rate=args.sample_rate,
        normalize=args.normalize,
        peak_target=args.peak,
    )

    print(f"wrote {args.out}")
    print(
        f"raw_peak={peak:.4f} scale={scale:.2f} "
        f"wind_min={min(controls):.2f} wind_max={max(controls):.2f}"
    )

    if args.play and not play_wav(args.out):
        print("no supported audio player found; open the WAV manually", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
