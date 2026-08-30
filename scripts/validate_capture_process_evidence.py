#!/usr/bin/env python3
"""Validate one capture process against its canonical, process-bound run log."""

from __future__ import annotations

import argparse
import re
import shutil
import sys
from pathlib import Path

from runtime_log_diagnostics import first_fatal_diagnostic


LOG_TIME = r"(?:[01]\d|2[0-3]):[0-5]\d:[0-5]\d\.\d{3}"


def production_log_line(level: str, module: str, payload: str) -> re.Pattern[str]:
    return re.compile(
        rf"^\[{LOG_TIME} {level} {re.escape(module)}\] {payload}$",
        re.MULTILINE,
    )


RUN_LOG_MARKER = production_log_line(
    "INFO", "re_flora", r"\[RUN_LOG\] path=(?P<path>.+?)"
)
PUBLICATION = production_log_line(
    "INFO",
    "re_flora::app::core::environment_lighting_test_scene",
    r"\[ENV_LIGHT_TEST\] static terrain ready .*?terrain_revision=(\d+).*",
)
INITIALIZATION = production_log_line(
    "INFO",
    "re_flora::tracer",
    r"\[DDGI\] initialization requested terrain_revision=(\d+).*",
)
VERIFICATION = production_log_line(
    "INFO",
    "re_flora::app::core::environment_lighting_test_scene",
    r"\[ENV_LIGHT_TEST\] first DDGI build verified .*?geometry_revision=(\d+) "
    r"visible_terrain_publication_revision=(\d+).*",
)
CAPTURE_SAVED = production_log_line(
    "INFO",
    "re_flora::app::core::environment_irradiance_capture",
    r"\[ENV_IRRADIANCE_CAPTURE\] saved\b.*",
)
CAPTURE_COMPLETE = production_log_line(
    "INFO",
    "re_flora::app::core",
    r"\[ENV_IRRADIANCE_CAPTURE\] complete; exiting one-shot capture run",
)


def exactly_one(pattern: re.Pattern[str], text: str, label: str) -> re.Match[str]:
    matches = list(pattern.finditer(text))
    if len(matches) != 1:
        raise ValueError(f"expected exactly one {label}, found {len(matches)}")
    return matches[0]


def validate(console_path: Path, *, require_test_scene_startup: bool) -> Path:
    console = console_path.read_text(encoding="utf-8", errors="replace")
    marker = exactly_one(RUN_LOG_MARKER, console, "process-bound [RUN_LOG] marker")
    run_log_path = Path(marker.group("path"))
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
    if Path(log_marker.group("path")) != canonical_run_log:
        raise ValueError("console and run-log [RUN_LOG] markers disagree")

    for label, text in (("console", console), ("run log", run_log)):
        diagnostic = first_fatal_diagnostic(text)
        if diagnostic is not None:
            raise ValueError(
                f"{label} contains fatal or validation diagnostic: {diagnostic.group(0).strip()}"
            )

        saved = exactly_one(CAPTURE_SAVED, text, f"{label} capture saved event")
        complete = exactly_one(
            CAPTURE_COMPLETE, text, f"{label} capture completion event"
        )
        if saved.start() >= complete.start():
            raise ValueError(f"{label} capture completion precedes capture save")

    if require_test_scene_startup:
        for label, text in (("console", console), ("run log", run_log)):
            publication = exactly_one(
                PUBLICATION, text, f"{label} test-scene terrain publication"
            )
            initialization = exactly_one(
                INITIALIZATION, text, f"{label} first DDGI initialization"
            )
            verification = exactly_one(
                VERIFICATION, text, f"{label} first DDGI build verification"
            )
            revisions = (
                int(publication.group(1)),
                int(initialization.group(1)),
                int(verification.group(1)),
                int(verification.group(2)),
            )
            if len(set(revisions)) != 1:
                raise ValueError(
                    f"{label} test-scene publication and first DDGI build revisions differ: "
                    f"publication={revisions[0]} initialization={revisions[1]} "
                    f"build={revisions[2]} verified_publication={revisions[3]}"
                )
            if not (publication.start() < initialization.start() < verification.start()):
                raise ValueError(
                    f"{label} test-scene Visible Terrain Publication must precede first "
                    "DDGI initialization and typed build verification"
                )
    return canonical_run_log


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--require-test-scene-startup", action="store_true")
    parser.add_argument("--preserve-run-log", type=Path)
    parser.add_argument("console", type=Path)
    args = parser.parse_args()
    try:
        run_log = validate(
            args.console,
            require_test_scene_startup=args.require_test_scene_startup,
        )
    except (OSError, ValueError) as error:
        print(f"capture process evidence invalid: {error}", file=sys.stderr)
        return 1
    preserved = None
    if args.preserve_run_log is not None:
        try:
            destination = args.preserve_run_log.absolute()
            destination.parent.mkdir(parents=True, exist_ok=True)
            if destination != run_log:
                shutil.copy2(run_log, destination)
            preserved = destination
        except OSError as error:
            print(
                f"capture process evidence invalid: cannot preserve bound run log: {error}",
                file=sys.stderr,
            )
            return 1
    suffix = f" preserved_run_log={preserved}" if preserved is not None else ""
    print(
        f"[CAPTURE_PROCESS] console={args.console} run_log={run_log}{suffix} status=PASS"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
