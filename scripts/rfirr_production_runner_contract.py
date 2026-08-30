#!/usr/bin/env python3
"""Tripwire for normalized production calls into the current RFIRR CLI.

This deliberately validates a narrow source form; it does not interpret arbitrary shell.
The current-only CLI remains the schema seal for every argv it receives.
"""

from __future__ import annotations

import re
from collections import Counter


CURRENT_ENTRY = "analyze_current_environment_irradiance_capture.py"
COMPATIBILITY_ENTRY = "analyze_environment_irradiance_capture.py"
FUNCTION_INVOCATION = re.compile(
    r"^\s*(?:(?:if|elif)\b.*?\s!\s+)?analyze_current_capture(?:\s|$)"
)
SHELL_FUNCTION = re.compile(
    r"^\s*(?:function\s+([A-Za-z_][A-Za-z0-9_]*)"
    r"(?:\s*\(\s*\))?|([A-Za-z_][A-Za-z0-9_]*)\s*\(\s*\))\s*\{\s*$"
)
CURRENT_FUNCTION_DEFINITION = re.compile(
    r"^\s*(?:function\s+analyze_current_capture(?:\s*\(\s*\))?(?:\s|$)"
    r"|analyze_current_capture\s*\(\s*\))"
)
CURRENT_FUNCTION_OVERRIDE = re.compile(
    r"^\s*(?:"
    r"alias\s+analyze_current_capture\s*="
    r"|(?:export\s+|readonly\s+|declare\s+|typeset\s+)?"
    r"analyze_current_capture\s*="
    r"|unset\s+-f\s+analyze_current_capture(?:\s|$)"
    r")"
)
TOP_LEVEL = "<top-level>"
RUNNER_INVOCATION_INVENTORY: dict[str, dict[str, int]] = {
    "check_ddgi_correctness.sh": {TOP_LEVEL: 8},
    "check_ddgi_inflight_terrain_edits.sh": {"run_case": 1, TOP_LEVEL: 1},
    "check_ddgi_lifecycle_acceptance.sh": {
        "check_radiance": 1,
        "check_density": 1,
    },
    "check_ddgi_local_terrain_convergence.sh": {TOP_LEVEL: 1},
    "check_ddgi_runtime_terrain_edits.sh": {
        "check_captures": 1,
        "check_inflight_stale_active_captures": 1,
        "check_flora_consumer": 1,
    },
    "check_ddgi_terrain_edit_cycle.sh": {"run_case": 2},
    "check_ddgi_transport_acceptance.sh": {"execute_analysis": 1},
}
RUNNER_PRODUCTION_DEPENDENCIES: dict[str, frozenset[str]] = {
    "check_ddgi_correctness.sh": frozenset(
        {"scripts/analyze_current_environment_irradiance_capture.py"}
    ),
    "check_ddgi_inflight_terrain_edits.sh": frozenset(
        {"scripts/analyze_current_environment_irradiance_capture.py"}
    ),
    "check_ddgi_lifecycle_acceptance.sh": frozenset(
        {
            "scripts/analyze_current_environment_irradiance_capture.py",
            "scripts/validate_ddgi_radiance_lifecycle.py",
        }
    ),
    "check_ddgi_local_terrain_convergence.sh": frozenset(
        {"scripts/analyze_current_environment_irradiance_capture.py"}
    ),
    "check_ddgi_runtime_terrain_edits.sh": frozenset(
        {"scripts/analyze_current_environment_irradiance_capture.py"}
    ),
    "check_ddgi_terrain_edit_cycle.sh": frozenset(
        {"scripts/analyze_current_environment_irradiance_capture.py"}
    ),
    "check_ddgi_transport_acceptance.sh": frozenset(
        {
            "scripts/analyze_current_environment_irradiance_capture.py",
            "scripts/check_ddgi_correctness.sh",
            "scripts/check_ddgi_lifecycle_acceptance.sh",
            "scripts/check_ddgi_runtime_terrain_edits.sh",
            "scripts/check_ddgi_sky_normalization_evidence.py",
            "scripts/summarize_ddgi_convergence.py",
        }
    ),
}
PRODUCTION_SCRIPT_REFERENCE = re.compile(
    r"\$(?:repo_root|\{repo_root\})/(scripts/[A-Za-z0-9_.-]+\.(?:py|sh))"
)
CANONICAL_FUNCTION_BODY = (
    re.compile(r"^if\s+\$dry_run\s*;\s*then$"),
    re.compile(
        r"^printf\s+'%q '\s+analyze_current_capture\s+\"\$@\"\s+>&2$"
    ),
    re.compile(r"^printf\s+'\\n'\s+>&2$"),
    re.compile(r"^return\s+0$"),
    re.compile(r"^fi$"),
    re.compile(
        r'^"\$(?:repo_root|\{repo_root\})/scripts/'
        r'analyze_current_environment_irradiance_capture\.py"\s+"\$@"$'
    ),
)
TRANSPORT_EXECUTION_CALL = re.compile(
    r'^if\s+!\s+execute_analysis\s+"\$json"\s+'
    r'"\$\{arguments\[@\]\}"\s*;\s*then$'
)


def production_runner_invocation_failures(
    runner_name: str, source: str
) -> list[str]:
    """Require the normalized seal and the runner's explicit call-site inventory."""
    lines = source.splitlines()
    failures: list[str] = []
    definitions = [
        index
        for index, line in enumerate(lines)
        if CURRENT_FUNCTION_DEFINITION.match(line) is not None
    ]
    sealed = len(definitions) == 1 and _canonical_function_is_sealed(
        lines, definitions[0]
    )
    overrides = [
        line for line in lines if CURRENT_FUNCTION_OVERRIDE.match(line) is not None
    ]
    if (
        len(definitions) != 1
        or not sealed
        or overrides
        or source.count(CURRENT_ENTRY) != 1
    ):
        failures.append("runner lacks the unique direct current-schema function seal")

    expected_inventory = RUNNER_INVOCATION_INVENTORY.get(runner_name)
    if expected_inventory is None:
        failures.append(f"runner has no declared invocation inventory: {runner_name}")
    elif _invocation_inventory(lines) != expected_inventory:
        failures.append(
            "runner current-schema invocation inventory differs from "
            f"{expected_inventory}"
        )
    if runner_name == "check_ddgi_transport_acceptance.sh" and not (
        _scope_lines(lines, "run_analysis")
        and sum(
            TRANSPORT_EXECUTION_CALL.fullmatch(line.strip()) is not None
            for line in _scope_lines(lines, "run_analysis")
        )
        == 1
    ):
        failures.append("transport runner lacks its shared analysis execution seam")
    expected_dependencies = RUNNER_PRODUCTION_DEPENDENCIES.get(runner_name)
    actual_dependencies = frozenset(PRODUCTION_SCRIPT_REFERENCE.findall(source))
    if expected_dependencies is None:
        failures.append(f"runner has no declared dependency inventory: {runner_name}")
    elif actual_dependencies != expected_dependencies:
        failures.append(
            "runner production evidence dependencies differ from "
            f"{sorted(expected_dependencies)}"
        )
    if COMPATIBILITY_ENTRY in source:
        failures.append("runner names the compatibility analyzer")
    if "--expect-version" in source:
        failures.append("runner exposes RFIRR version selection")
    return failures


def production_evidence_dependencies(sources: dict[str, str]) -> frozenset[str]:
    """Return the declared transitive closure for the seven production runners."""
    pending = list(sources)
    visited: set[str] = set()
    dependencies: set[str] = {f"scripts/{name}" for name in sources}
    while pending:
        runner_name = pending.pop()
        if runner_name in visited:
            continue
        visited.add(runner_name)
        declared = RUNNER_PRODUCTION_DEPENDENCIES.get(runner_name, frozenset())
        dependencies.update(declared)
        for dependency in declared:
            child_name = dependency.removeprefix("scripts/")
            if child_name in sources and child_name not in visited:
                pending.append(child_name)
    return frozenset(dependencies)


def _invocation_inventory(lines: list[str]) -> dict[str, int]:
    counts: Counter[str] = Counter()
    function_scope: str | None = None
    for line in lines:
        function_name = _shell_function_name(line)
        if function_name is not None:
            function_scope = function_name
            continue
        if line == "}" and function_scope is not None:
            function_scope = None
            continue
        if (
            function_scope != "analyze_current_capture"
            and not line.lstrip().startswith("#")
            and FUNCTION_INVOCATION.match(line) is not None
        ):
            counts[function_scope or TOP_LEVEL] += 1
    return dict(counts)


def _shell_function_name(line: str) -> str | None:
    function = SHELL_FUNCTION.fullmatch(line)
    if function is None:
        return None
    return function.group(1) or function.group(2)


def _scope_lines(lines: list[str], scope: str) -> list[str]:
    inside = False
    body: list[str] = []
    for line in lines:
        function_name = _shell_function_name(line)
        if function_name is not None:
            inside = function_name == scope
            continue
        if inside and re.fullmatch(r"\s*}\s*", line):
            return body
        if inside:
            body.append(line)
    return []


def _canonical_function_is_sealed(lines: list[str], definition: int) -> bool:
    body: list[str] = []
    for line in lines[definition + 1 :]:
        if re.fullmatch(r"\s*}\s*", line):
            break
        if line.strip() and not line.lstrip().startswith("#"):
            body.append(line.strip())
    return len(body) == len(CANONICAL_FUNCTION_BODY) and all(
        pattern.fullmatch(statement) is not None
        for pattern, statement in zip(CANONICAL_FUNCTION_BODY, body, strict=True)
    )
