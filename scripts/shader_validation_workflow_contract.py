#!/usr/bin/env python3
"""Validate shader CI routing and executable Fedora DDGI evidence steps."""

from __future__ import annotations

import fnmatch


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
        if fnmatch.fnmatchcase(path, candidate):
            routed = not excluded
    return routed


def _step_blocks(lines: list[str], job: str) -> list[list[str]]:
    job_block = _mapping_block(lines, job, 2)
    if _statically_disabled(job_block, 4):
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


def _field_is_true(lines: list[str], key: str, indent: int) -> bool:
    field = _field(lines, key, indent)
    if field is None or len(field[1]) != 1:
        return False
    value = field[1][0].strip('"\'').lower()
    return value in {"true", "${{ true }}"}


def _statically_disabled(lines: list[str], indent: int) -> bool:
    field = _field(lines, "if", indent)
    if field is None or len(field[1]) != 1:
        return False
    value = field[1][0].strip('"\'').lower()
    return value in {"false", "${{ false }}"}


def _step_single_command(step: list[str]) -> str | None:
    if _statically_disabled(step, 8):
        return None
    if _field_is_true(step, "continue-on-error", 8):
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

    fedora_commands = {
        command
        for step in _step_blocks(lines, "fedora")
        if (command := _step_single_command(step)) is not None
    }
    for command in REQUIRED_FEDORA_COMMANDS:
        if command not in fedora_commands:
            failures.append(f"Fedora job does not run {command}")
    return failures
