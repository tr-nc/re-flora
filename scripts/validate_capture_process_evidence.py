#!/usr/bin/env python3
"""Validate one capture process against its canonical, process-bound run log."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


RUN_LOG_MARKER = re.compile(r"\[RUN_LOG\] path=(\S+)")
FATAL_DIAGNOSTIC = re.compile(
    r"\bERROR\b|\bvalidation\s+(?:error|failure)\b|\bpanic(?:ked)?\b|"
    r"VUID-|\bdevice\s+lost\b|\bstale\s+readback\b",
    re.IGNORECASE | re.MULTILINE,
)
PUBLICATION = re.compile(
    r"\[ENV_LIGHT_TEST\] static terrain ready .*?terrain_revision=(\d+)"
)
INITIALIZATION = re.compile(
    r"\[DDGI\] initialization requested terrain_revision=(\d+)"
)
VERIFICATION = re.compile(
    r"\[ENV_LIGHT_TEST\] first DDGI build verified .*?geometry_revision=(\d+) "
    r"visible_terrain_publication_revision=(\d+)"
)


def exactly_one(pattern: re.Pattern[str], text: str, label: str) -> re.Match[str]:
    matches = list(pattern.finditer(text))
    if len(matches) != 1:
        raise ValueError(f"expected exactly one {label}, found {len(matches)}")
    return matches[0]


def validate(console_path: Path) -> Path:
    console = console_path.read_text(encoding="utf-8", errors="replace")
    marker = exactly_one(RUN_LOG_MARKER, console, "process-bound [RUN_LOG] marker")
    run_log_path = Path(marker.group(1))
    if not run_log_path.is_absolute():
        raise ValueError(f"process-bound run log path is not absolute: {run_log_path}")
    try:
        canonical_run_log = run_log_path.resolve(strict=True)
    except OSError as error:
        raise ValueError(f"process-bound run log is unavailable: {run_log_path}: {error}") from error
    if canonical_run_log != run_log_path:
        raise ValueError(
            f"process-bound run log marker is not canonical: {run_log_path} != {canonical_run_log}"
        )
    run_log = canonical_run_log.read_text(encoding="utf-8", errors="replace")
    log_marker = exactly_one(RUN_LOG_MARKER, run_log, "run-log [RUN_LOG] marker")
    if Path(log_marker.group(1)) != canonical_run_log:
        raise ValueError("console and run-log [RUN_LOG] markers disagree")

    for label, text in (("console", console), ("run log", run_log)):
        diagnostic = FATAL_DIAGNOSTIC.search(text)
        if diagnostic is not None:
            raise ValueError(
                f"{label} contains fatal or validation diagnostic: {diagnostic.group(0).strip()}"
            )

    publication = exactly_one(PUBLICATION, console, "test-scene terrain publication")
    initialization = exactly_one(INITIALIZATION, console, "first DDGI initialization")
    verification = exactly_one(VERIFICATION, console, "first DDGI build verification")
    revisions = (
        int(publication.group(1)),
        int(initialization.group(1)),
        int(verification.group(1)),
        int(verification.group(2)),
    )
    if len(set(revisions)) != 1:
        raise ValueError(
            "test-scene publication and first DDGI build revisions differ: "
            f"publication={revisions[0]} initialization={revisions[1]} "
            f"build={revisions[2]} verified_publication={revisions[3]}"
        )
    if not (publication.start() < initialization.start() < verification.start()):
        raise ValueError(
            "test-scene Visible Terrain Publication must precede first DDGI initialization "
            "and typed build verification"
        )
    return canonical_run_log


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("console", type=Path)
    args = parser.parse_args()
    try:
        run_log = validate(args.console)
    except (OSError, ValueError) as error:
        print(f"capture process evidence invalid: {error}", file=sys.stderr)
        return 1
    print(f"[CAPTURE_PROCESS] console={args.console} run_log={run_log} status=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
