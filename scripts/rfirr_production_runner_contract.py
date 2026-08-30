#!/usr/bin/env python3
"""Tripwire for normalized production calls into the current RFIRR CLI.

This deliberately validates a narrow source form; it does not interpret arbitrary shell.
The current-only CLI remains the schema seal for every argv it receives.
"""

from __future__ import annotations

import re


CURRENT_ENTRY = "analyze_current_environment_irradiance_capture.py"
COMPATIBILITY_ENTRY = "analyze_environment_irradiance_capture.py"
FUNCTION_HEADER = "analyze_current_capture() {"
DIRECT_CALL = (
    '    "$repo_root/scripts/analyze_current_environment_irradiance_capture.py" "$@"'
)
FUNCTION_END = "}"
FUNCTION_INVOCATION = re.compile(
    r"^\s*(?:(?:if|elif)\b.*?\s!\s+)?analyze_current_capture(?:\s|$)"
)


def production_runner_invocation_failures(source: str) -> list[str]:
    """Require one normalized seal and at least one command-position invocation."""
    lines = source.splitlines()
    failures: list[str] = []
    definitions = [index for index, line in enumerate(lines) if line == FUNCTION_HEADER]
    sealed = any(
        lines[index : index + 3] == [FUNCTION_HEADER, DIRECT_CALL, FUNCTION_END]
        for index in definitions
    )
    if len(definitions) != 1 or not sealed or source.count(CURRENT_ENTRY) != 1:
        failures.append("runner lacks the unique direct current-schema function seal")

    invocations = [
        line
        for line in lines
        if not line.lstrip().startswith("#")
        and FUNCTION_INVOCATION.match(line) is not None
    ]
    if not invocations:
        failures.append("runner has no command-position current-schema invocation")
    if COMPATIBILITY_ENTRY in source:
        failures.append("runner names the compatibility analyzer")
    if "--expect-version" in source:
        failures.append("runner exposes RFIRR version selection")
    return failures
