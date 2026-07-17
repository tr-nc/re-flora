#!/usr/bin/env python3
"""Run spirv-val over the newest re-flora precompiled shader artifact set."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from pathlib import Path


def newest_artifact_root(target: Path) -> Path:
    roots = list(target.glob("debug/build/re-flora-vkn-*/out/precompiled-shaders"))
    if not roots:
        raise RuntimeError(f"no precompiled shader artifacts found under {target}")
    return max(roots, key=lambda path: path.stat().st_mtime_ns)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", type=Path, default=Path("target"))
    parser.add_argument("--expected", type=int)
    parser.add_argument("--validator", default="spirv-val")
    args = parser.parse_args()

    validator = shutil.which(args.validator)
    if validator is None:
        parser.error(f"validator not found on PATH: {args.validator}")

    root = newest_artifact_root(args.target)
    artifacts = sorted(root.rglob("*.spv"))
    if args.expected is not None and len(artifacts) != args.expected:
        raise RuntimeError(
            f"expected {args.expected} SPIR-V artifacts under {root}, found {len(artifacts)}"
        )
    for artifact in artifacts:
        subprocess.run(
            [validator, "--target-env", "vulkan1.3", artifact],
            check=True,
        )
    print(f"validated {len(artifacts)} SPIR-V artifacts under {root}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
