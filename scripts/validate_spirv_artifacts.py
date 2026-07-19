#!/usr/bin/env python3
"""Run spirv-val over the newest re-flora precompiled shader artifact set."""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from collections import Counter
from pathlib import Path

GENERATED_ARTIFACT_RE = re.compile(
    r'include_bytes!\(concat!\(env!\("OUT_DIR"\), "(/precompiled-shaders/[^"\n]+\.spv)"\)\)'
)


def newest_artifact_root(target: Path) -> Path:
    roots = list(target.glob("debug/build/re-flora-vkn-*/out/precompiled-shaders"))
    if not roots:
        raise RuntimeError(f"no precompiled shader artifacts found under {target}")
    return max(roots, key=lambda path: path.stat().st_mtime_ns)


def generated_artifact_paths(root: Path) -> set[Path]:
    generated = root.parent / "precompiled_shaders.rs"
    try:
        source = generated.read_text(encoding="utf-8")
    except OSError as error:
        raise RuntimeError(
            f"could not read generated shader inventory {generated}: {error}"
        ) from error

    paths = GENERATED_ARTIFACT_RE.findall(source)
    if not paths:
        raise RuntimeError(f"no SPIR-V artifacts referenced by {generated}")

    duplicate_paths = sorted(
        path for path, count in Counter(paths).items() if count != 1
    )
    if duplicate_paths:
        raise RuntimeError(
            f"duplicate SPIR-V artifact references in {generated}: {duplicate_paths}"
        )

    prefix = "/precompiled-shaders/"
    return {Path(path.removeprefix(prefix)) for path in paths}


def validated_artifact_paths(root: Path) -> list[Path]:
    expected = generated_artifact_paths(root)
    actual = {path.relative_to(root) for path in root.rglob("*.spv")}
    missing = sorted(expected - actual)
    unexpected = sorted(actual - expected)
    if missing or unexpected:
        details = [
            "SPIR-V artifact inventory does not match "
            f"{root.parent / 'precompiled_shaders.rs'}"
        ]
        if missing:
            details.append(
                f"missing ({len(missing)}): {[path.as_posix() for path in missing]}"
            )
        if unexpected:
            details.append(
                f"unexpected ({len(unexpected)}): {[path.as_posix() for path in unexpected]}"
            )
        raise RuntimeError("\n".join(details))
    return [root / path for path in sorted(expected)]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", type=Path, default=Path("target"))
    parser.add_argument("--validator", default="spirv-val")
    args = parser.parse_args()

    validator = shutil.which(args.validator)
    if validator is None:
        parser.error(f"validator not found on PATH: {args.validator}")

    root = newest_artifact_root(args.target)
    artifacts = validated_artifact_paths(root)
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
