#!/usr/bin/env python3
"""Measure practical spectral/envelope features for tree-rustle WAVs.

The prototype deliberately has no heavy Python audio dependencies, so this
script uses only the standard library. It focuses on the bands called out in
``docs/procedural_tree_rustle_progress.md`` rather than producing studio-grade
analysis:

- low/mid wind bed: 200-3000 Hz
- leaf-contact band: 3000-6000 Hz
- restrained high air: 8000-12000 Hz

Example:

    cd tools
    uv run python analyze_tree_rustle.py ../docs/audio/wind_ref.wav \
      --section clean_wind 2.4 8.8 \
      --section leaf_rise 8.8 16.6
"""

from __future__ import annotations

import argparse
import json
import math
import struct
import wave
from collections.abc import Iterable, Sequence
from dataclasses import asdict, dataclass
from pathlib import Path

TWO_PI = math.tau
EPSILON = 1.0e-12
BANDS: tuple[tuple[str, float, float], ...] = (
    ("low_mid_200_3000", 200.0, 3000.0),
    ("leaf_3000_6000", 3000.0, 6000.0),
    ("air_8000_12000", 8000.0, 12000.0),
)


@dataclass(frozen=True)
class WavAudio:
    sample_rate: int
    channels: int
    left: list[float]
    right: list[float]

    @property
    def duration_seconds(self) -> float:
        return len(self.left) / self.sample_rate


@dataclass(frozen=True)
class SectionMetrics:
    name: str
    start_seconds: float
    end_seconds: float
    duration_seconds: float
    rms_dbfs: float
    peak_dbfs: float
    stereo_correlation: float
    stereo_width_ratio: float
    envelope_p10_dbfs: float
    envelope_p50_dbfs: float
    envelope_p90_dbfs: float
    envelope_range_db: float
    spectral_centroid_hz: float
    spectral_rolloff_95_hz: float
    band_dbfs: dict[str, float]
    band_ratios_db: dict[str, float]


def dbfs_from_power(power: float) -> float:
    return 10.0 * math.log10(max(power, EPSILON))


def dbfs_from_amplitude(amplitude: float) -> float:
    return 20.0 * math.log10(max(abs(amplitude), EPSILON))


def clamp(value: float, low: float, high: float) -> float:
    return min(high, max(low, value))


def lowpass_alpha(cutoff_hz: float, sample_rate: int) -> float:
    cutoff_hz = clamp(cutoff_hz, 1.0, sample_rate * 0.45)
    return 1.0 - math.exp(-TWO_PI * cutoff_hz / sample_rate)


def read_wav(path: Path) -> WavAudio:
    with wave.open(str(path), "rb") as reader:
        channels = reader.getnchannels()
        sample_width = reader.getsampwidth()
        sample_rate = reader.getframerate()
        frame_count = reader.getnframes()
        if channels < 1:
            raise ValueError(f"{path} has no channels")
        if sample_width not in {1, 2, 3, 4}:
            raise ValueError(f"unsupported sample width for {path}: {sample_width} bytes")
        frames = reader.readframes(frame_count)

    left: list[float] = []
    right: list[float] = []
    frame_size = channels * sample_width
    for offset in range(0, len(frames), frame_size):
        samples = [decode_pcm(frames, offset + channel * sample_width, sample_width) for channel in range(channels)]
        left_value = samples[0]
        right_value = samples[1] if channels > 1 else samples[0]
        left.append(left_value)
        right.append(right_value)
    return WavAudio(sample_rate=sample_rate, channels=channels, left=left, right=right)


def decode_pcm(data: bytes, offset: int, sample_width: int) -> float:
    if sample_width == 1:
        return (data[offset] - 128) / 128.0
    if sample_width == 2:
        return struct.unpack_from("<h", data, offset)[0] / 32768.0
    if sample_width == 3:
        raw = data[offset : offset + 3]
        sign = b"\xff" if raw[2] & 0x80 else b"\x00"
        return int.from_bytes(raw + sign, "little", signed=True) / 8388608.0
    return struct.unpack_from("<i", data, offset)[0] / 2147483648.0


def mono_samples(audio: WavAudio, start: int, end: int) -> list[float]:
    return [(left + right) * 0.5 for left, right in zip(audio.left[start:end], audio.right[start:end])]


def mean_power(samples: Sequence[float]) -> float:
    if not samples:
        return 0.0
    return sum(sample * sample for sample in samples) / len(samples)


def peak_amplitude(samples: Sequence[float]) -> float:
    return max((abs(sample) for sample in samples), default=0.0)


def bandpass_power(samples: Sequence[float], sample_rate: int, low_hz: float, high_hz: float) -> float:
    """Estimate band power with cascaded one-pole high/low filters.

    The filter is intentionally simple and deterministic. Cascading twice gives
    enough separation for prototype tuning while keeping the script dependency
    free.
    """
    hp_alpha = lowpass_alpha(low_hz, sample_rate)
    lp_alpha = lowpass_alpha(high_hz, sample_rate)
    hp_state_1 = hp_state_2 = 0.0
    lp_state_1 = lp_state_2 = 0.0
    power = 0.0
    warmup = min(len(samples) // 8, int(0.050 * sample_rate))
    count = 0
    for index, sample in enumerate(samples):
        hp_state_1 += (sample - hp_state_1) * hp_alpha
        high_1 = sample - hp_state_1
        hp_state_2 += (high_1 - hp_state_2) * hp_alpha
        high_2 = high_1 - hp_state_2
        lp_state_1 += (high_2 - lp_state_1) * lp_alpha
        lp_state_2 += (lp_state_1 - lp_state_2) * lp_alpha
        if index >= warmup:
            power += lp_state_2 * lp_state_2
            count += 1
    return power / max(1, count)


def frame_powers(samples: Sequence[float], sample_rate: int) -> list[float]:
    frame_size = max(1, int(0.050 * sample_rate))
    hop = max(1, int(0.025 * sample_rate))
    if len(samples) < frame_size:
        return [mean_power(samples)]
    powers = []
    for start in range(0, len(samples) - frame_size + 1, hop):
        powers.append(mean_power(samples[start : start + frame_size]))
    return powers


def percentile(values: Sequence[float], percent: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    position = clamp(percent, 0.0, 100.0) / 100.0 * (len(ordered) - 1)
    lower = int(math.floor(position))
    upper = int(math.ceil(position))
    if lower == upper:
        return ordered[lower]
    blend = position - lower
    return ordered[lower] * (1.0 - blend) + ordered[upper] * blend


def stereo_metrics(audio: WavAudio, start: int, end: int) -> tuple[float, float]:
    left = audio.left[start:end]
    right = audio.right[start:end]
    if not left:
        return 0.0, 0.0

    mid_power = side_power = 0.0
    left_power = right_power = cross_power = 0.0
    for left_sample, right_sample in zip(left, right):
        mid = (left_sample + right_sample) * 0.5
        side = (left_sample - right_sample) * 0.5
        mid_power += mid * mid
        side_power += side * side
        left_power += left_sample * left_sample
        right_power += right_sample * right_sample
        cross_power += left_sample * right_sample
    correlation = cross_power / math.sqrt(max(left_power * right_power, EPSILON))
    width_ratio = math.sqrt(side_power / max(mid_power, EPSILON))
    return correlation, width_ratio


def fft_real_magnitudes(frame: Sequence[float]) -> list[float]:
    """Return positive-bin magnitudes for a power-of-two real frame."""
    n = len(frame)
    data = [complex(value, 0.0) for value in frame]

    # Bit-reversal permutation.
    j = 0
    for i in range(1, n):
        bit = n >> 1
        while j & bit:
            j ^= bit
            bit >>= 1
        j ^= bit
        if i < j:
            data[i], data[j] = data[j], data[i]

    length = 2
    while length <= n:
        half = length // 2
        angle = -TWO_PI / length
        phase_step = complex(math.cos(angle), math.sin(angle))
        for start in range(0, n, length):
            phase = 1.0 + 0.0j
            for offset in range(half):
                even = data[start + offset]
                odd = data[start + offset + half] * phase
                data[start + offset] = even + odd
                data[start + offset + half] = even - odd
                phase *= phase_step
        length *= 2

    return [abs(value) for value in data[: n // 2 + 1]]


def spectral_shape(samples: Sequence[float], sample_rate: int) -> tuple[float, float]:
    if not samples:
        return 0.0, 0.0

    fft_size = 4096
    hop = max(1, int(0.25 * sample_rate))
    if len(samples) < fft_size:
        padded = list(samples) + [0.0] * (fft_size - len(samples))
        starts = [0]
    else:
        padded = samples
        starts = list(range(0, len(samples) - fft_size + 1, hop))
        if not starts:
            starts = [0]

    window = [0.5 - 0.5 * math.cos(TWO_PI * index / (fft_size - 1)) for index in range(fft_size)]
    powers = [0.0 for _ in range(fft_size // 2 + 1)]
    for start in starts:
        frame = [padded[start + index] * window[index] for index in range(fft_size)]
        for index, magnitude in enumerate(fft_real_magnitudes(frame)):
            powers[index] += magnitude * magnitude

    total_power = sum(powers)
    if total_power <= EPSILON:
        return 0.0, 0.0

    bin_hz = sample_rate / fft_size
    centroid = sum(index * bin_hz * power for index, power in enumerate(powers)) / total_power
    threshold = total_power * 0.95
    cumulative = 0.0
    rolloff = 0.0
    for index, power in enumerate(powers):
        cumulative += power
        if cumulative >= threshold:
            rolloff = index * bin_hz
            break
    return centroid, rolloff


def analyze_section(audio: WavAudio, name: str, start_seconds: float, end_seconds: float) -> SectionMetrics:
    start_seconds = clamp(start_seconds, 0.0, audio.duration_seconds)
    end_seconds = clamp(end_seconds, start_seconds, audio.duration_seconds)
    start = int(round(start_seconds * audio.sample_rate))
    end = int(round(end_seconds * audio.sample_rate))
    samples = mono_samples(audio, start, end)
    powers = frame_powers(samples, audio.sample_rate)
    envelope_db = [dbfs_from_power(power) for power in powers]
    p10 = percentile(envelope_db, 10.0)
    p50 = percentile(envelope_db, 50.0)
    p90 = percentile(envelope_db, 90.0)
    band_dbfs = {
        name: dbfs_from_power(bandpass_power(samples, audio.sample_rate, low_hz, high_hz))
        for name, low_hz, high_hz in BANDS
    }
    centroid, rolloff = spectral_shape(samples, audio.sample_rate)
    correlation, width_ratio = stereo_metrics(audio, start, end)
    return SectionMetrics(
        name=name,
        start_seconds=start_seconds,
        end_seconds=end_seconds,
        duration_seconds=end_seconds - start_seconds,
        rms_dbfs=dbfs_from_power(mean_power(samples)),
        peak_dbfs=dbfs_from_amplitude(peak_amplitude(samples)),
        stereo_correlation=correlation,
        stereo_width_ratio=width_ratio,
        envelope_p10_dbfs=p10,
        envelope_p50_dbfs=p50,
        envelope_p90_dbfs=p90,
        envelope_range_db=p90 - p10,
        spectral_centroid_hz=centroid,
        spectral_rolloff_95_hz=rolloff,
        band_dbfs=band_dbfs,
        band_ratios_db={
            "leaf_minus_low_mid": band_dbfs["leaf_3000_6000"] - band_dbfs["low_mid_200_3000"],
            "air_minus_leaf": band_dbfs["air_8000_12000"] - band_dbfs["leaf_3000_6000"],
            "air_minus_low_mid": band_dbfs["air_8000_12000"] - band_dbfs["low_mid_200_3000"],
        },
    )


def parse_sections(raw_sections: Iterable[Sequence[str]], duration_seconds: float) -> list[tuple[str, float, float]]:
    sections = []
    for raw in raw_sections:
        name, start_raw, end_raw = raw
        start = float(start_raw)
        end = float(end_raw)
        if end <= start:
            raise ValueError(f"section {name} end must be greater than start")
        sections.append((name, start, end))
    if not sections:
        sections.append(("full", 0.0, duration_seconds))
    return sections


def print_report(path: Path, audio: WavAudio, metrics: Sequence[SectionMetrics]) -> None:
    print(f"file: {path}")
    print(
        f"format: {audio.sample_rate} Hz, {audio.channels} ch, "
        f"duration={audio.duration_seconds:.2f}s"
    )
    for metric in metrics:
        print(f"\n[{metric.name}] {metric.start_seconds:.2f}-{metric.end_seconds:.2f}s")
        print(
            f"  rms={metric.rms_dbfs:.2f} dBFS peak={metric.peak_dbfs:.2f} dBFS "
            f"env_p10/p50/p90={metric.envelope_p10_dbfs:.2f}/"
            f"{metric.envelope_p50_dbfs:.2f}/{metric.envelope_p90_dbfs:.2f} dBFS"
        )
        print(
            f"  centroid={metric.spectral_centroid_hz:.0f} Hz "
            f"rolloff95={metric.spectral_rolloff_95_hz:.0f} Hz "
            f"stereo_corr={metric.stereo_correlation:.2f} width={metric.stereo_width_ratio:.2f}"
        )
        print(
            "  bands: "
            f"low/mid={metric.band_dbfs['low_mid_200_3000']:.2f} dBFS, "
            f"3-6k={metric.band_dbfs['leaf_3000_6000']:.2f} dBFS, "
            f"8-12k={metric.band_dbfs['air_8000_12000']:.2f} dBFS"
        )
        print(
            "  ratios: "
            f"3-6k minus low/mid={metric.band_ratios_db['leaf_minus_low_mid']:.2f} dB, "
            f"8-12k minus 3-6k={metric.band_ratios_db['air_minus_leaf']:.2f} dB"
        )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Analyze tree-rustle WAV spectral bands and envelopes.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("wav", type=Path)
    parser.add_argument(
        "--section",
        nargs=3,
        action="append",
        default=[],
        metavar=("NAME", "START_SECONDS", "END_SECONDS"),
        help="analyze a named time section; may be repeated",
    )
    parser.add_argument("--json", action="store_true", help="print JSON instead of a text report")
    parser.add_argument("--write-json", type=Path, help="also write JSON metrics to this path")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    audio = read_wav(args.wav)
    sections = parse_sections(args.section, audio.duration_seconds)
    metrics = [analyze_section(audio, name, start, end) for name, start, end in sections]
    payload = {
        "file": str(args.wav),
        "sample_rate": audio.sample_rate,
        "channels": audio.channels,
        "duration_seconds": audio.duration_seconds,
        "sections": [asdict(metric) for metric in metrics],
    }

    if args.write_json:
        args.write_json.parent.mkdir(parents=True, exist_ok=True)
        args.write_json.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    if args.json:
        print(json.dumps(payload, indent=2))
    else:
        print_report(args.wav, audio, metrics)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
