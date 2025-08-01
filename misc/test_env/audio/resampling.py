#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import shutil
import sys
from pathlib import Path
from typing import Generator

import numpy as np
import soxr
import soundfile as sf

TARGET_SR = 48_000           # resample target sample-rate (Hz)
TARGET_DBFS = -1.0           # audacity style peak level
OUTPUT_DIR_NAME = "game_audio"


def find_wav_files(root_dir: Path) -> Generator[Path, None, None]:
    for path in root_dir.rglob("*.wav"):
        yield path


def prepare_output_path(input_path: Path, root_dir: Path, output_root: Path) -> Path:
    return output_root / input_path.relative_to(root_dir)


def select_left_channel(data: np.ndarray) -> np.ndarray:
    if data.ndim == 1 or data.shape[1] == 1:
        return data.squeeze()
    return data[:, 0]


def resample_audio(data: np.ndarray, original_sr: int, target_sr: int) -> np.ndarray:
    if original_sr == target_sr:
        return data
    return soxr.resample(data, original_sr, target_sr, quality="VHQ")


def normalize_audio(data: np.ndarray, target_db: float = TARGET_DBFS) -> np.ndarray:
    """
    audacity-style normalization:
    1. remove dc offset
    2. scale peak to target_db (linear value 10**(db/20))
    """
    # remove dc offset
    centered = data - np.mean(data)
    # peak-normalize
    peak_linear = 10 ** (target_db / 20.0)
    max_abs = np.max(np.abs(centered))
    if max_abs == 0:
        return centered
    gain = peak_linear / max_abs
    return centered * gain


def process_file(input_path: Path, output_path: Path, target_sr: int = TARGET_SR) -> None:
    data, sr = sf.read(input_path, always_2d=True)
    mono = select_left_channel(data)
    resampled = resample_audio(mono, sr, target_sr)
    normalized = normalize_audio(resampled)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    sf.write(output_path, normalized, target_sr)
    print(f"converted: {input_path} -> {output_path}")


def delete_previous_output(output_root: Path) -> None:
    if output_root.exists() and output_root.is_dir():
        shutil.rmtree(output_root)
        print(f"removed previous {output_root}")


def main() -> None:
    script_dir = Path(__file__).parent.resolve()
    output_root = script_dir / OUTPUT_DIR_NAME

    delete_previous_output(output_root)

    wav_files = [p for p in find_wav_files(script_dir) if OUTPUT_DIR_NAME not in p.parts]
    if not wav_files:
        print("no .wav files found")
        sys.exit(0)

    for input_path in wav_files:
        output_path = prepare_output_path(input_path, script_dir, output_root)
        try:
            process_file(input_path, output_path)
        except Exception as exc:  # pylint: disable=broad-except
            print(f"error processing {input_path}: {exc}", file=sys.stderr)


if __name__ == "__main__":
    main()