#!/usr/bin/env python3
"""Record the computer's playback audio, not the microphone.

On Linux/PipeWire/PulseAudio this records from the monitor source of the
current default output sink. In other words, it captures whatever the computer
is playing through speakers/headphones.

Examples:

    cd tools
    uv run python record_system_audio.py --duration 30
    uv run python record_system_audio.py --out ../target/audio-captures/wind_ref.wav
    uv run python record_system_audio.py --list-devices

If --duration is omitted, recording continues until Ctrl+C.
"""

from __future__ import annotations

import argparse
import datetime as dt
import math
import shutil
import signal
import subprocess
import sys
import time
import wave
from array import array
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT_DIR = ROOT / "target" / "audio-captures"
DEFAULT_SAMPLE_RATE = 48_000
DEFAULT_CHANNELS = 2
BYTES_PER_SAMPLE = 2  # s16le


class RecorderError(RuntimeError):
    pass


def run_text(command: list[str]) -> str:
    try:
        return subprocess.check_output(command, text=True, stderr=subprocess.DEVNULL).strip()
    except (OSError, subprocess.CalledProcessError):
        return ""


def default_sink() -> str:
    return run_text(["pactl", "get-default-sink"])


def default_monitor() -> str:
    sink = default_sink()
    if sink:
        return f"{sink}.monitor"
    return "@DEFAULT_MONITOR@"


def list_sources() -> list[tuple[str, str, str, str]]:
    output = run_text(["pactl", "list", "short", "sources"])
    sources: list[tuple[str, str, str, str]] = []
    for line in output.splitlines():
        parts = line.split("\t")
        if len(parts) >= 5:
            index, name, _driver, sample_spec, state = parts[:5]
            sources.append((index, name, sample_spec, state))
    return sources


def print_devices() -> None:
    sink = default_sink()
    monitor = default_monitor()
    print(f"default sink: {sink or '(unknown)'}")
    print(f"default monitor source: {monitor}")
    print()
    print("monitor sources (system playback capture):")
    for index, name, sample_spec, state in list_sources():
        if not name.endswith(".monitor"):
            continue
        marker = "*" if name == monitor else " "
        print(f"{marker} {index:>4}  {state:<9}  {sample_spec:<18}  {name}")
    print()
    print("non-monitor sources are microphones/inputs and are intentionally not used by default.")


def timestamped_output_path() -> Path:
    stamp = dt.datetime.now().strftime("%Y%m%d_%H%M%S")
    return DEFAULT_OUTPUT_DIR / f"system_audio_{stamp}.wav"


def wav_duration(path: Path) -> float:
    with wave.open(str(path), "rb") as reader:
        frames = reader.getnframes()
        rate = reader.getframerate()
    return frames / rate if rate else 0.0


def chunk_meter(data: bytes) -> tuple[float, float]:
    if not data:
        return 0.0, 0.0
    samples = array("h")
    usable = len(data) - (len(data) % BYTES_PER_SAMPLE)
    samples.frombytes(data[:usable])
    if sys.byteorder != "little":
        samples.byteswap()
    if not samples:
        return 0.0, 0.0
    peak = max(abs(sample) for sample in samples) / 32768.0
    # Sampling the whole chunk is cheap at these chunk sizes and keeps the meter simple.
    rms = math.sqrt(sum(sample * sample for sample in samples) / len(samples)) / 32768.0
    return peak, rms


def stop_process(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    try:
        process.send_signal(signal.SIGTERM)
        process.wait(timeout=2.0)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=2.0)


def record_system_audio(
    *,
    output_path: Path,
    device: str,
    duration_seconds: float | None,
    sample_rate: int,
    channels: int,
    quiet: bool,
    progress_every: float,
) -> None:
    if shutil.which("parec") is None:
        raise RecorderError("parec was not found; install PulseAudio/PipeWire PulseAudio tools")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    bytes_per_frame = channels * BYTES_PER_SAMPLE
    frames_limit = None
    if duration_seconds is not None:
        frames_limit = max(1, int(round(duration_seconds * sample_rate)))

    command = [
        "parec",
        "--record",
        "--raw",
        "--format=s16le",
        f"--rate={sample_rate}",
        f"--channels={channels}",
        f"--device={device}",
        "--client-name=re-flora-system-audio-recorder",
        "--stream-name=system playback capture",
    ]

    if not quiet:
        print(f"recording system playback from: {device}")
        print(f"writing: {output_path}")
        if duration_seconds is None:
            print("duration: until Ctrl+C")
        else:
            print(f"duration: {duration_seconds:.2f}s")
        print("source type: monitor source (computer output), not microphone")

    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if process.stdout is None:
        stop_process(process)
        raise RecorderError("failed to open parec stdout")

    frames_written = 0
    chunk_frames = max(256, sample_rate // 10)
    chunk_bytes = chunk_frames * bytes_per_frame
    last_progress = time.monotonic()

    try:
        with wave.open(str(output_path), "wb") as writer:
            writer.setnchannels(channels)
            writer.setsampwidth(BYTES_PER_SAMPLE)
            writer.setframerate(sample_rate)

            while True:
                if frames_limit is not None:
                    remaining_frames = frames_limit - frames_written
                    if remaining_frames <= 0:
                        break
                    read_size = min(chunk_bytes, remaining_frames * bytes_per_frame)
                else:
                    read_size = chunk_bytes

                data = process.stdout.read(read_size)
                if not data:
                    break

                # Keep the WAV frame-aligned even if the pipe returns a partial sample.
                usable = len(data) - (len(data) % bytes_per_frame)
                if usable <= 0:
                    continue
                data = data[:usable]
                writer.writeframes(data)
                frames_written += usable // bytes_per_frame

                now = time.monotonic()
                if not quiet and progress_every > 0.0 and now - last_progress >= progress_every:
                    peak, rms = chunk_meter(data)
                    elapsed = frames_written / sample_rate
                    print(f"recorded={elapsed:7.2f}s peak={peak:0.3f} rms={rms:0.3f}", flush=True)
                    last_progress = now
    except KeyboardInterrupt:
        if not quiet:
            print("\nstopping capture")
    finally:
        stop_process(process)

    stderr = b""
    if process.stderr is not None:
        try:
            stderr = process.stderr.read()
        except OSError:
            stderr = b""

    if frames_written == 0:
        message = stderr.decode(errors="replace").strip()
        raise RecorderError(message or "no audio frames were captured")

    if not quiet:
        duration = frames_written / sample_rate
        print(f"wrote {output_path}")
        print(f"captured={duration:.2f}s sample_rate={sample_rate} channels={channels}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Record computer playback audio from the default output monitor source, not the microphone.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--duration", type=float, help="seconds to record; omit to record until Ctrl+C")
    parser.add_argument("--out", type=Path, default=None, help="output WAV path")
    parser.add_argument(
        "--device",
        default="@DEFAULT_MONITOR@",
        help="PulseAudio/PipeWire source; keep @DEFAULT_MONITOR@ to capture current system output",
    )
    parser.add_argument("--sample-rate", type=int, default=DEFAULT_SAMPLE_RATE)
    parser.add_argument("--channels", type=int, default=DEFAULT_CHANNELS)
    parser.add_argument("--quiet", action="store_true")
    parser.add_argument("--progress-every", type=float, default=1.0, help="seconds between progress meter lines; 0 disables")
    parser.add_argument("--list-devices", action="store_true", help="list monitor sources and exit")
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()

    if args.list_devices:
        print_devices()
        return 0
    if args.duration is not None and args.duration <= 0.0:
        parser.error("--duration must be positive")
    if args.sample_rate < 8_000:
        parser.error("--sample-rate must be at least 8000")
    if args.channels not in {1, 2}:
        parser.error("--channels must be 1 or 2")

    output_path = args.out or timestamped_output_path()
    try:
        record_system_audio(
            output_path=output_path,
            device=str(args.device),
            duration_seconds=args.duration,
            sample_rate=args.sample_rate,
            channels=args.channels,
            quiet=bool(args.quiet),
            progress_every=max(0.0, float(args.progress_every)),
        )
    except RecorderError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    try:
        duration = wav_duration(output_path)
        if duration <= 0.0:
            print("warning: output WAV has zero duration", file=sys.stderr)
    except wave.Error as error:
        print(f"warning: could not inspect WAV: {error}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
