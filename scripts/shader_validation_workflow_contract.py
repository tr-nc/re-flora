#!/usr/bin/env python3
"""Validate shader CI routing and executable Fedora DDGI evidence steps."""

from __future__ import annotations

import re


REQUIRED_OWNER_PATHS = (
    ".github/workflows/shader-validation.yml",
    "docs/ddgi_indirect_transport_spec.md",
    "docs/ddgi_transport_acceptance.md",
    "scripts/analyze_environment_irradiance_capture.py",
    "scripts/analyze_current_environment_irradiance_capture.py",
    "scripts/check_ddgi_correctness.sh",
    "scripts/check_ddgi_inflight_terrain_edits.sh",
    "scripts/check_ddgi_lifecycle_acceptance.sh",
    "scripts/check_ddgi_local_terrain_convergence.sh",
    "scripts/check_ddgi_runtime_terrain_edits.sh",
    "scripts/check_ddgi_sky_normalization_evidence.py",
    "scripts/check_ddgi_terrain_edit_cycle.sh",
    "scripts/check_ddgi_transport_acceptance.sh",
    "scripts/ddgi_evidence/__init__.py",
    "scripts/ddgi_evidence/cli.py",
    "scripts/ddgi_evidence/executor.py",
    "scripts/ddgi_evidence/model.py",
    "scripts/ddgi_evidence/plan.py",
    "scripts/ddgi_evidence/validation.py",
    "scripts/shader_validation_workflow_contract.py",
    "scripts/summarize_ddgi_convergence.py",
    "scripts/tests/test_ddgi_evidence_cli.py",
    "scripts/tests/test_ddgi_evidence_plan.py",
    "scripts/tests/test_ddgi_evidence_validation.py",
    "scripts/tests/test_shader_validation_workflow.py",
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


def _mapping_entry(
    line: str, indent: int, *, list_item: bool = False
) -> tuple[str, str] | None:
    if _indent(line) != indent or not line.strip() or line.lstrip().startswith("#"):
        return None
    content = line.strip()
    if list_item:
        if not content.startswith("- "):
            return None
        content = content[2:].lstrip()
    if content.startswith(("\"", "'")):
        quote = content[0]
        closing = content.find(quote, 1)
        if closing == -1 or not content[closing + 1 :].lstrip().startswith(":"):
            return None
        key = content[1:closing]
        value = content[closing + 1 :].lstrip()[1:].strip()
        return key, value
    key, separator, value = content.partition(":")
    if not separator or not key:
        return None
    return key.strip(), value.strip()


def _mapping_block(lines: list[str], key: str, indent: int) -> list[str]:
    for index, line in enumerate(lines):
        entry = _mapping_entry(line, indent)
        if entry == (key, ""):
            end = index + 1
            while end < len(lines):
                candidate = lines[end]
                if candidate.strip() and not candidate.lstrip().startswith("#"):
                    if _indent(candidate) <= indent:
                        break
                end += 1
            return lines[index + 1 : end]
    return []


def _mapping_blocks(lines: list[str], key: str, indent: int) -> list[list[str]]:
    blocks: list[list[str]] = []
    for index, line in enumerate(lines):
        if _mapping_entry(line, indent) != (key, ""):
            continue
        end = index + 1
        while end < len(lines):
            candidate = lines[end]
            if candidate.strip() and not candidate.lstrip().startswith("#"):
                if _indent(candidate) <= indent:
                    break
            end += 1
        blocks.append(lines[index + 1 : end])
    return blocks


def _route_patterns(on_block: list[str], event: str) -> tuple[str, ...]:
    event_blocks = _mapping_blocks(on_block, event, 2)
    if len(event_blocks) != 1:
        return ()
    paths_blocks = _mapping_blocks(event_blocks[0], "paths", 4)
    if len(paths_blocks) != 1:
        return ()
    paths_block = paths_blocks[0]
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
    """Match the fail-closed workflow glob subset used by this repository."""
    if not _supported_route_pattern(pattern):
        return False
    expression: list[str] = ["^"]
    index = 0
    while index < len(pattern):
        character = pattern[index]
        if character == "*":
            if index + 1 < len(pattern) and pattern[index + 1] == "*":
                if index + 2 < len(pattern) and pattern[index + 2] == "/":
                    expression.append("(?:.*/)?")
                    index += 3
                else:
                    expression.append(".*")
                    index += 2
            else:
                expression.append("[^/]*")
                index += 1
        else:
            expression.append(re.escape(character))
            index += 1
    expression.append("$")
    return re.match("".join(expression), path) is not None


def _supported_route_pattern(pattern: str) -> bool:
    candidate = pattern[1:] if pattern.startswith("!") else pattern
    return bool(
        candidate
        and "***" not in candidate
        and re.fullmatch(r"[A-Za-z0-9._/*-]+", candidate)
    )


def _step_blocks(jobs_block: list[str], job: str) -> list[list[str]]:
    job_blocks = _mapping_blocks(jobs_block, job, 2)
    if len(job_blocks) != 1:
        return []
    job_block = job_blocks[0]
    container = _field(job_block, "container", 4)
    if (
        container != ("scalar", ("fedora:43",))
        or any(
            _field(job_block, field, 4) is not None
            for field in ("if", "continue-on-error", "env")
        )
        or _has_default_shell(job_block, 4)
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
    for index, line in enumerate(lines):
        entry = _mapping_entry(line, indent)
        if entry is None or entry[0] != key:
            continue
        value = entry[1].split("#", 1)[0].strip()
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
    if _step_keys(step) != ("name", "run"):
        return None
    run = _field(step, "run", 8)
    if run is None or run[0] != "scalar" or len(run[1]) != 1:
        return None
    return run[1][0]


def _step_keys(step: list[str]) -> tuple[str, ...]:
    keys: list[str] = []
    for index, line in enumerate(step):
        entry = _mapping_entry(
            line,
            6 if index == 0 else 8,
            list_item=index == 0,
        )
        if entry is not None:
            keys.append(entry[0])
    return tuple(keys)


def _global_environment_is_safe(lines: list[str]) -> bool:
    environment = _mapping_block(lines, "env", 0)
    entries: list[tuple[str, str]] = []
    for line in environment:
        entry = _mapping_entry(line, 2)
        if entry is None:
            if line.strip() and not line.lstrip().startswith("#"):
                return False
            continue
        key, value = entry
        if not key:
            return False
        entries.append((key, value.strip().strip('"\'')))
    return entries == [("CARGO_TERM_COLOR", "always")]


def workflow_contract_failures(source: str) -> list[str]:
    lines = source.splitlines()
    failures: list[str] = []
    on_blocks = _mapping_blocks(lines, "on", 0)
    if len(on_blocks) != 1:
        failures.append("workflow must have one root on mapping")
    on_block = on_blocks[0] if len(on_blocks) == 1 else []
    for event in ("pull_request", "push"):
        event_blocks = _mapping_blocks(on_block, event, 2)
        if len(event_blocks) != 1:
            failures.append(f"root on must have one {event} mapping")
        patterns = _route_patterns(on_block, event)
        for pattern in patterns:
            if not _supported_route_pattern(pattern):
                failures.append(f"{event} uses unsupported route pattern {pattern}")
        for owner_path in REQUIRED_OWNER_PATHS:
            if not _routes(patterns, owner_path):
                failures.append(f"{event} does not route {owner_path}")

    jobs_blocks = _mapping_blocks(lines, "jobs", 0)
    if len(jobs_blocks) != 1:
        failures.append("workflow must have one root jobs mapping")
    jobs_block = jobs_blocks[0] if len(jobs_blocks) == 1 else []
    if len(_mapping_blocks(jobs_block, "fedora", 2)) != 1:
        failures.append("root jobs must have one fedora mapping")
    fedora_commands = [
        command
        for step in _step_blocks(jobs_block, "fedora")
        if (command := _step_single_command(step)) is not None
    ]
    if _has_default_shell(lines, 0) or not _global_environment_is_safe(lines):
        fedora_commands = []
    for command in REQUIRED_FEDORA_COMMANDS:
        if fedora_commands.count(command) != 1:
            failures.append(f"Fedora job does not run {command}")
    return failures
