#!/usr/bin/env python3
"""Compile and run every repository Slang CPU test with the pinned compiler."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from install_slang import SLANG_VERSION


ROOT = Path(__file__).resolve().parents[1]
TEST_ROOT = ROOT / "shader" / "tests"
MODULE_ROOT = ROOT / "shader" / "slang"
SLANG_VERSION_TIMEOUT_SECONDS = 30
SLANG_COMPILE_TIMEOUT_SECONDS = 120
SLANG_TEST_TIMEOUT_SECONDS = 30


def find_slangc() -> Path:
    configured = os.environ.get("SLANGC")
    candidate = Path(configured) if configured else None
    if candidate is None:
        discovered = shutil.which("slangc")
        candidate = Path(discovered) if discovered else None
    if candidate is None or not candidate.is_file():
        raise RuntimeError(
            "slangc was not found; set SLANGC or install the pinned compiler "
            "with scripts/install_slang.py"
        )

    result = subprocess.run(
        [candidate, "-version"],
        check=True,
        capture_output=True,
        text=True,
        timeout=SLANG_VERSION_TIMEOUT_SECONDS,
    )
    version = (result.stdout + result.stderr).strip()
    if SLANG_VERSION not in version:
        raise RuntimeError(
            f"expected Slang {SLANG_VERSION}, got: {version or '<no version output>'}"
        )
    return candidate


def main() -> int:
    slangc = find_slangc()
    tests = sorted(TEST_ROOT.glob("*.slang"))
    if not tests:
        raise RuntimeError(f"no Slang CPU tests found under {TEST_ROOT}")

    with tempfile.TemporaryDirectory(prefix="re-flora-slang-tests-") as temporary:
        output_root = Path(temporary)
        for source in tests:
            executable = output_root / source.stem
            if os.name == "nt":
                executable = executable.with_suffix(".exe")
            subprocess.run(
                [
                    slangc,
                    source,
                    "-std",
                    "2025",
                    "-I",
                    MODULE_ROOT,
                    "-target",
                    "executable",
                    "-o",
                    executable,
                ],
                cwd=ROOT,
                check=True,
                timeout=SLANG_COMPILE_TIMEOUT_SECONDS,
            )
            print(f"running {source.relative_to(ROOT).as_posix()}", flush=True)
            subprocess.run(
                [executable], cwd=ROOT, check=True, timeout=SLANG_TEST_TIMEOUT_SECONDS
            )

    print(f"all {len(tests)} Slang CPU tests passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        raise SystemExit(1)
