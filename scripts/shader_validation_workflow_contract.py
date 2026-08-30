#!/usr/bin/env python3
"""Validate shader CI routing and executable Fedora DDGI evidence steps."""

from __future__ import annotations

import re


REQUIRED_OWNER_PATHS = (
    "scripts/analyze_environment_irradiance_capture.py",
    "scripts/shader_validation_workflow_contract.py",
    "scripts/summarize_ddgi_convergence.py",
    "scripts/validate_ddgi_radiance_lifecycle.py",
    "src/app/core/ddgi_spatial_weight_readback.rs",
    "src/app/core/environment_irradiance_capture.rs",
    "src/app/core/environment_lighting_test_scene.rs",
    "src/app/core/environment_lighting_test_scene/local_light_scaling.rs",
    "src/app/core/mod.rs",
    "src/cli.rs",
    "src/ddgi/capture.rs",
    "src/ddgi/resources.rs",
    "src/ddgi/runtime.rs",
    "src/environment_lighting.rs",
    "src/tracer/buffer_updater.rs",
    "src/tracer/pipeline_builder.rs",
)
REQUIRED_FEDORA_COMMANDS = (
    "cargo test --locked capture_metadata_uses_authoritative_published_terminal_identity",
    "cargo test --locked ddgi::resources::tests::filter_",
    "python3 -m unittest scripts.tests.test_analyze_environment_irradiance_capture.AnalyzeEnvironmentIrradianceCaptureTests.test_rust_producer_v10_golden_decodes_with_exact_filter_witness",
)


def _indent(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def _mapping_block(lines: list[str], key: str, indent: int) -> list[str]:
    header = f"{' ' * indent}{key}:"
    for index, line in enumerate(lines):
        if line == header:
            end = index + 1
            while end < len(lines):
                candidate = lines[end]
                if candidate.strip() and not candidate.lstrip().startswith("#"):
                    if _indent(candidate) <= indent:
                        break
                end += 1
            return lines[index + 1 : end]
    return []


def _route_patterns(lines: list[str], event: str) -> tuple[str, ...]:
    event_block = _mapping_block(lines, event, 2)
    paths_block = _mapping_block(event_block, "paths", 4)
    patterns: list[str] = []
    for line in paths_block:
        if _indent(line) != 6:
            continue
        item = line.strip()
        if not item.startswith("- "):
            continue
        patterns.append(item[2:].strip().strip('"\''))
    return tuple(patterns)


def _routes(patterns: tuple[str, ...], path: str) -> bool:
    routed = False
    for pattern in patterns:
        excluded = pattern.startswith("!")
        candidate = pattern[1:] if excluded else pattern
        if _github_path_matches(candidate, path):
            routed = not excluded
    return routed


def _github_path_matches(pattern: str, path: str) -> bool:
    """Match the slash semantics used by GitHub workflow path filters."""
    expression: list[str] = ["^"]
    index = 0
    while index < len(pattern):
        character = pattern[index]
        if character == "*":
            if index + 1 < len(pattern) and pattern[index + 1] == "*":
                expression.append(".*")
                index += 2
            else:
                expression.append("[^/]*")
                index += 1
        elif character == "?":
            expression.append("[^/]")
            index += 1
        else:
            expression.append(re.escape(character))
            index += 1
    expression.append("$")
    return re.match("".join(expression), path) is not None


def _step_blocks(lines: list[str], job: str) -> list[list[str]]:
    job_block = _mapping_block(lines, job, 2)
    if _field(job_block, "if", 4) is not None or _has_default_shell(
        job_block, 4
    ):
        return []
    steps_block = _mapping_block(job_block, "steps", 4)
    steps: list[list[str]] = []
    current: list[str] | None = None
    for line in steps_block:
        if _indent(line) == 6 and line.strip().startswith("- "):
            if current is not None:
                steps.append(current)
            current = [line]
        elif current is not None:
            current.append(line)
    if current is not None:
        steps.append(current)
    return steps


def _field(
    lines: list[str], key: str, indent: int
) -> tuple[str, tuple[str, ...]] | None:
    prefix = f"{key}:"
    for index, line in enumerate(lines):
        if _indent(line) != indent or not line.strip().startswith(prefix):
            continue
        value = line.strip().split(":", 1)[1].split("#", 1)[0].strip()
        if value not in {"|", ">", "|-", ">-"}:
            return "scalar", ((value.strip('"\''),) if value else ())
        values: list[str] = []
        for block_line in lines[index + 1 :]:
            if block_line.strip() and _indent(block_line) <= indent:
                break
            block_value = block_line.strip()
            if block_value and not block_value.startswith("#"):
                values.append(block_value)
        return "block", tuple(values)
    return None


def _has_default_shell(lines: list[str], indent: int) -> bool:
    defaults = _mapping_block(lines, "defaults", indent)
    run_defaults = _mapping_block(defaults, "run", indent + 2)
    return _field(run_defaults, "shell", indent + 4) is not None


def _step_single_command(step: list[str]) -> str | None:
    if any(
        _field(step, field, 8) is not None
        for field in ("if", "continue-on-error", "shell")
    ):
        return None
    run = _field(step, "run", 8)
    if run is None or run[0] != "scalar" or len(run[1]) != 1:
        return None
    return run[1][0]


def workflow_contract_failures(source: str) -> list[str]:
    lines = source.splitlines()
    failures: list[str] = []
    for event in ("pull_request", "push"):
        patterns = _route_patterns(lines, event)
        for owner_path in REQUIRED_OWNER_PATHS:
            if not _routes(patterns, owner_path):
                failures.append(f"{event} does not route {owner_path}")

    fedora_commands = [
        command
        for step in _step_blocks(lines, "fedora")
        if (command := _step_single_command(step)) is not None
    ]
    if _has_default_shell(lines, 0):
        fedora_commands = []
    for command in REQUIRED_FEDORA_COMMANDS:
        if fedora_commands.count(command) != 1:
            failures.append(f"Fedora job does not run {command}")
    return failures
