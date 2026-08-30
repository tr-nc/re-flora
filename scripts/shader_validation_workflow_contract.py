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
    return any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns)


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


def _statically_disabled(lines: list[str], indent: int) -> bool:
    for line in lines:
        if _indent(line) != indent or not line.strip().startswith("if:"):
            continue
        value = line.strip().split(":", 1)[1].split("#", 1)[0].strip()
        if value.lower() in {"false", "'false'", '"false"', "${{ false }}"}:
            return True
    return False


def _step_run_lines(step: list[str]) -> tuple[str, ...]:
    if _statically_disabled(step, 8):
        return ()
    for index, line in enumerate(step):
        if _indent(line) != 8 or not line.strip().startswith("run:"):
            continue
        value = line.strip().split(":", 1)[1].strip()
        if value not in {"|", ">", "|-", ">-"}:
            return (value.strip('"\''),) if value else ()
        commands: list[str] = []
        for block_line in step[index + 1 :]:
            if block_line.strip() and _indent(block_line) <= 8:
                break
            command = block_line.strip()
            if command and not command.startswith("#"):
                commands.append(command)
        return tuple(commands)
    return ()


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
        for command in _step_run_lines(step)
    }
    for command in REQUIRED_FEDORA_COMMANDS:
        if command not in fedora_commands:
            failures.append(f"Fedora job does not run {command}")
    return failures
