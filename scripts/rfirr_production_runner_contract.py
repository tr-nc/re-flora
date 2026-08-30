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
ANALYZER_IDENTIFIER = re.compile(r"\banalyze_current_capture\b")
DRY_RUN_IDENTIFIER = re.compile(r"\bdry_run\b")
DRY_RUN_EXPANSION = re.compile(r"\$(?:dry_run\b|\{dry_run(?:[^}]*)\})")
REPO_ROOT_IDENTIFIER = re.compile(r"\brepo_root\b")
REPO_ROOT_EXPANSION = re.compile(r"\$(?:repo_root\b|\{repo_root\})")
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
CARGO_IDENTIFIER = re.compile(r"\bcargo\b")
APP_IDENTIFIER = re.compile(r"(?<![A-Za-z0-9_-])re-flora(?![A-Za-z0-9_-])")
CANONICAL_CARGO_BUILD = re.compile(
    r'^\s*command cargo build (?:--quiet )?--release --manifest-path "\$repo_root/Cargo\.toml"\s*$'
)
CANONICAL_CARGO_RUN = re.compile(
    r'^\s*command cargo run --quiet --release --manifest-path "\$repo_root/Cargo\.toml" --\s*$'
)
CANONICAL_REPO_ROOT = (
    'readonly repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"'
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
    dry_run_controlled = _dry_run_controlled_analysis_calls(runner_name, lines)
    if dry_run_controlled:
        failures.append(
            "runner current-schema analysis execution is controlled by dry_run at "
            + ", ".join(dry_run_controlled)
        )
    unknown_occurrences = _unclassified_analyzer_occurrences(lines)
    if unknown_occurrences:
        failures.append(
            "runner has unclassified analyzer occurrence at "
            + ", ".join(unknown_occurrences)
        )
    authority_failures, unknown_dry_run = _immutable_authority_failures(lines)
    if authority_failures:
        failures.append(
            "runner violates immutable runner authority at "
            + ", ".join(authority_failures)
        )
    if unknown_dry_run:
        failures.append(
            "runner has unclassified dry_run occurrence at "
            + ", ".join(unknown_dry_run)
        )
    if runner_name == "check_ddgi_transport_acceptance.sh":
        if not (
            _scope_lines(lines, "run_analysis")
            and sum(
                TRANSPORT_EXECUTION_CALL.fullmatch(line.strip()) is not None
                for line in _scope_lines(lines, "run_analysis")
            )
            == 1
        ):
            failures.append("transport runner lacks its shared analysis execution seam")
        if not _transport_sink_policy_is_sealed(lines):
            failures.append("transport runner lacks exact analyzer-to-sink policy")
    launch_failures = _process_launch_failures(lines)
    if launch_failures:
        failures.append(
            "runner has unauthorized process launch at " + ", ".join(launch_failures)
        )
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
    scopes = _function_scopes(lines)
    for line, function_scope in zip(lines, scopes, strict=True):
        if (
            _shell_function_name(line) is None
            and function_scope != "analyze_current_capture"
            and not line.lstrip().startswith("#")
            and _canonical_analyzer_invocation(line)
        ):
            counts[function_scope or TOP_LEVEL] += 1
    return dict(counts)


def _dry_run_controlled_analysis_calls(
    runner_name: str, lines: list[str]
) -> list[str]:
    """Reject canonical analysis execution nested in a dry-run conditional chain."""
    frames: list[dict[str, object]] = []
    recorded_calls: list[tuple[int, str | None, tuple[dict[str, object], ...]]] = []
    scopes = _function_scopes(lines)
    code_lines = _shell_code_lines(lines)
    previous_scope: str | None = None
    dry_run_token = DRY_RUN_EXPANSION
    analyzer_call = ANALYZER_IDENTIFIER
    transport_execution = re.compile(r"\bexecute_analysis(?:\s|$)")

    for line_number, (line, code, function_scope) in enumerate(
        zip(lines, code_lines, scopes, strict=True), 1
    ):
        if function_scope != previous_scope:
            frames = []
            previous_scope = function_scope
        stripped = code.strip()
        if not stripped or stripped.startswith("#"):
            continue

        if re.fullmatch(r"fi\s*;?", stripped):
            if frames:
                frames.pop()
            continue

        if re.match(r"^elif\b", stripped):
            if frames:
                frames[-1]["in_condition"] = not _has_then_token(stripped)
                if dry_run_token.search(stripped):
                    frames[-1]["dry_run"] = True
        elif re.match(r"^else(?:\s*;|\s*$)", stripped):
            pass
        elif re.match(r"^if\b", stripped):
            frames.append(
                {
                    "dry_run": dry_run_token.search(stripped) is not None,
                    "in_condition": not _has_then_token(stripped),
                }
            )
        elif frames and bool(frames[-1]["in_condition"]):
            if dry_run_token.search(stripped):
                frames[-1]["dry_run"] = True
            if _has_then_token(stripped):
                frames[-1]["in_condition"] = False

        is_analysis_call = analyzer_call.search(stripped) is not None
        if runner_name == "check_ddgi_transport_acceptance.sh":
            is_analysis_call = is_analysis_call or (
                transport_execution.search(stripped) is not None
            )
        if (
            _shell_function_name(line) is None
            and function_scope != "analyze_current_capture"
            and is_analysis_call
        ):
            recorded_calls.append((line_number, function_scope, tuple(frames)))

    violations: list[str] = []
    for line_number, scope, active_frames in recorded_calls:
        if any(bool(frame["dry_run"]) for frame in active_frames):
            violations.append(f"{scope or TOP_LEVEL}:{line_number}")
    return violations


def _canonical_analyzer_invocation(line: str) -> bool:
    code = _shell_code(line)
    occurrences = tuple(ANALYZER_IDENTIFIER.finditer(code))
    if len(occurrences) != 1:
        return False
    occurrence = occurrences[0]
    suffix = code[occurrence.end() :]
    if suffix and suffix[0] not in " \t<>|;&":
        return False
    prefix = code[: occurrence.start()].strip()
    if prefix in ("", "!"):
        return True
    if re.fullmatch(r"(?:if|elif)(?:\s+!)?", prefix):
        return True
    return (
        re.fullmatch(r"(?:if|elif)\s+.+(?:&&|\|\|)\s*!?", prefix)
        is not None
    )


def _unclassified_analyzer_occurrences(lines: list[str]) -> list[str]:
    scopes = _function_scopes(lines)
    unknown: list[str] = []
    for line_number, (line, scope) in enumerate(zip(lines, scopes, strict=True), 1):
        occurrences = tuple(ANALYZER_IDENTIFIER.finditer(line))
        if not occurrences:
            continue
        if _shell_function_name(line) == "analyze_current_capture":
            classified = len(occurrences) == 1
        elif scope == "analyze_current_capture":
            classified = True
        else:
            classified = len(occurrences) == 1 and _canonical_analyzer_invocation(line)
        if not classified:
            unknown.append(str(line_number))
    return unknown


def _immutable_authority_failures(
    lines: list[str],
) -> tuple[list[str], list[str]]:
    authority_failures: list[str] = []
    unknown_dry_run: list[str] = []

    root_definitions = [
        index
        for index, line in enumerate(lines)
        if line.strip() == CANONICAL_REPO_ROOT
    ]
    if root_definitions != [3]:
        authority_failures.append("repo_root-definition")

    dry_false = [index for index, line in enumerate(lines) if line.strip() == "dry_run=false"]
    dry_true = [index for index, line in enumerate(lines) if line.strip() == "dry_run=true"]
    dry_readonly = [
        index for index, line in enumerate(lines) if line.strip() == "readonly dry_run"
    ]
    if not (
        len(dry_false) == len(dry_true) == len(dry_readonly) == 1
        and dry_false[0] < dry_true[0] < dry_readonly[0]
        and _shell_code(lines[dry_readonly[0] - 1]).strip() == "fi"
    ):
        authority_failures.append("dry_run-lifecycle")

    for line_number, line in enumerate(lines, 1):
        dry_occurrences = tuple(DRY_RUN_IDENTIFIER.finditer(line))
        dry_expansions = tuple(DRY_RUN_EXPANSION.finditer(line))
        canonical_dry_line = line.strip() in {
            "dry_run=false",
            "dry_run=true",
            "readonly dry_run",
        }
        if dry_occurrences and not canonical_dry_line:
            covered = {
                occurrence.start()
                for expansion in dry_expansions
                for occurrence in dry_occurrences
                if expansion.start() <= occurrence.start() < expansion.end()
            }
            if len(covered) != len(dry_occurrences):
                authority_failures.append(f"dry_run:{line_number}")
                unknown_dry_run.append(str(line_number))

        root_occurrences = tuple(REPO_ROOT_IDENTIFIER.finditer(line))
        root_expansions = tuple(REPO_ROOT_EXPANSION.finditer(line))
        if root_occurrences and line.strip() != CANONICAL_REPO_ROOT:
            covered = {
                occurrence.start()
                for expansion in root_expansions
                for occurrence in root_occurrences
                if expansion.start() <= occurrence.start() < expansion.end()
            }
            if len(covered) != len(root_occurrences):
                authority_failures.append(f"repo_root:{line_number}")

    return authority_failures, unknown_dry_run


def _transport_sink_policy_is_sealed(lines: list[str]) -> bool:
    statements = [
        _shell_code(line).strip()
        for line in _scope_lines(lines, "execute_analysis")
        if _shell_code(line).strip()
    ]
    dry_sink = re.compile(r"local\s+sink=\(\s*cat\s*\)")
    production_sink = re.compile(
        r'sink=\(\s*command\s+tee\s+"\$json"\s*\)'
    )
    sink_assignments = [
        (index, statement)
        for index, statement in enumerate(statements)
        if re.match(r"^(?:local\s+)?sink\s*=", statement)
    ]
    if len(sink_assignments) != 2:
        return False
    if dry_sink.fullmatch(sink_assignments[0][1]) is None:
        return False
    if production_sink.fullmatch(sink_assignments[1][1]) is None:
        return False
    production_sink_index = sink_assignments[1][0]
    if production_sink_index == 0 or production_sink_index + 1 >= len(statements):
        return False
    if (
        re.fullmatch(
            r"if\s+!\s*\$dry_run\s*;\s*then",
            statements[production_sink_index - 1],
        )
        is None
    ):
        return False
    if statements[production_sink_index + 1] != "fi":
        return False
    pipelines = [
        statement
        for statement in statements
        if ANALYZER_IDENTIFIER.search(statement) is not None
    ]
    if len(pipelines) != 1:
        return False
    stages = _shell_pipeline_stages(pipelines[0])
    return (
        len(stages) == 2
        and re.fullmatch(r'analyze_current_capture\s+"\$@"', stages[0]) is not None
        and re.fullmatch(r'"\$\{sink\[@\]\}"', stages[1]) is not None
    )


def _shell_pipeline_stages(statement: str) -> tuple[str, ...]:
    stages: list[str] = []
    start = 0
    quote: str | None = None
    escaped = False
    for index, character in enumerate(statement):
        if escaped:
            escaped = False
            continue
        if character == "\\" and quote != "'":
            escaped = True
            continue
        if quote is not None:
            if character == quote:
                quote = None
            continue
        if character in "'\"":
            quote = character
        elif character == "|":
            if (
                (index > 0 and statement[index - 1] == "|")
                or (index + 1 < len(statement) and statement[index + 1] == "|")
            ):
                continue
            stages.append(statement[start:index].strip())
            start = index + 1
    stages.append(statement[start:].strip())
    return tuple(stages)


def _process_launch_failures(lines: list[str]) -> list[str]:
    failures: list[str] = []
    cargo_builds = 0
    cargo_runs = 0
    for line_number, line in enumerate(lines, 1):
        cargo_occurrences = tuple(CARGO_IDENTIFIER.finditer(line))
        if cargo_occurrences:
            if len(cargo_occurrences) != 1:
                failures.append(f"cargo:{line_number}")
            elif CANONICAL_CARGO_BUILD.fullmatch(line):
                cargo_builds += 1
                if not _cargo_build_is_non_dry_run(lines, line_number - 1):
                    failures.append(f"cargo-build-policy:{line_number}")
            elif CANONICAL_CARGO_RUN.fullmatch(line):
                cargo_runs += 1
            else:
                failures.append(f"cargo:{line_number}")
        if APP_IDENTIFIER.search(line) is not None:
            failures.append(f"re-flora:{line_number}")
    if cargo_builds != 1:
        failures.append(f"cargo-build-count:{cargo_builds}")
    if cargo_runs != 1:
        failures.append(f"cargo-run-count:{cargo_runs}")
    command_launches = [
        index
        for index, line in enumerate(lines)
        if '"${command[@]}"' in line
        and "print_command" not in line
        and not re.match(r"^\s*printf\b", line)
    ]
    if len(command_launches) != 1:
        failures.append(f"cargo-command-launch-count:{len(command_launches)}")
    elif not _command_array_launch_is_non_dry_run(lines, command_launches[0]):
        failures.append(f"cargo-command-policy:{command_launches[0] + 1}")
    return failures


def _cargo_build_is_non_dry_run(lines: list[str], line_index: int) -> bool:
    significant = [
        _shell_code(line).strip()
        for line in lines[max(0, line_index - 5) : line_index]
        if _shell_code(line).strip()
    ]
    reference = r"(?:\$dry_run|\$\{dry_run(?:[^}]*)\})"
    non_dry_if = re.compile(rf"if\s+!\s*{reference}\s*;\s*then")
    dry_if = re.compile(rf"if\s+{reference}\s*;\s*then")
    for reverse_index, statement in enumerate(reversed(significant)):
        if non_dry_if.fullmatch(statement):
            return True
        if statement == "fi" or dry_if.fullmatch(statement):
            return False
        if statement == "else":
            earlier = significant[: len(significant) - reverse_index - 1]
            return any(dry_if.fullmatch(candidate) for candidate in reversed(earlier))
    return False


def _command_array_launch_is_non_dry_run(
    lines: list[str], line_index: int
) -> bool:
    if _non_dry_run_branch_lines(lines)[line_index]:
        return True
    scopes = _function_scopes(lines)
    scope = scopes[line_index]
    start = max(0, line_index - 18)
    prefix = "\n".join(
        _shell_code(lines[index]).strip()
        for index in range(start, line_index)
        if scopes[index] == scope
    )
    return (
        re.search(
            r"if\s+(?:\$dry_run|\$\{dry_run(?:[^}]*)\})\s*;\s*then\b"
            r"(?:(?!\bfi\b).)*\b(?:return\s+0|continue)\b"
            r"(?:(?!\bfi\b).)*\bfi\b",
            prefix,
            re.DOTALL,
        )
        is not None
    )


def _non_dry_run_branch_lines(lines: list[str]) -> list[bool]:
    policies: list[str] = []
    result: list[bool] = []
    reference = r"(?:\$dry_run|\$\{dry_run(?:[^}]*)\})"
    dry = re.compile(rf"if\s+{reference}\s*;\s*then")
    non_dry = re.compile(rf"if\s+!\s*{reference}\s*;\s*then")
    for code in _shell_code_lines(lines):
        statement = code.strip()
        if re.fullmatch(r"fi\s*;?", statement):
            if policies:
                policies.pop()
            result.append(any(policy == "non-dry" for policy in policies))
            continue
        if re.match(r"^elif\b", statement):
            if policies:
                policies[-1] = "unknown"
        elif re.fullmatch(r"else\s*;?", statement):
            if policies:
                policies[-1] = {
                    "dry": "non-dry",
                    "non-dry": "dry",
                }.get(policies[-1], "unknown")
        elif re.match(r"^if\b", statement):
            if non_dry.fullmatch(statement):
                policies.append("non-dry")
            elif dry.fullmatch(statement):
                policies.append("dry")
            else:
                policies.append("unknown")
        result.append(any(policy == "non-dry" for policy in policies))
    return result


def _has_then_token(statement: str) -> bool:
    return re.search(r"(?:^|[;\s])then(?:$|[;\s])", statement) is not None


def _shell_code(line: str) -> str:
    quote: str | None = None
    escaped = False
    for index, character in enumerate(line):
        if escaped:
            escaped = False
            continue
        if character == "\\" and quote != "'":
            escaped = True
            continue
        if quote is not None:
            if character == quote:
                quote = None
            continue
        if character in "'\"":
            quote = character
        elif character == "#" and (index == 0 or line[index - 1].isspace()):
            return line[:index]
    return line


def _shell_code_lines(lines: list[str]) -> list[str]:
    """Mask bodies of the repository's multiline single-quoted awk programs."""
    code_lines: list[str] = []
    in_multiline_single_quote = False
    for line in lines:
        quote_offsets = [
            index
            for index, character in enumerate(line)
            if character == "'" and (index == 0 or line[index - 1] != "\\")
        ]
        if in_multiline_single_quote:
            if len(quote_offsets) % 2 == 1:
                in_multiline_single_quote = False
                code_lines.append(_shell_code(line[quote_offsets[-1] + 1 :]))
            else:
                code_lines.append("")
            continue
        if len(quote_offsets) % 2 == 1:
            in_multiline_single_quote = True
            code_lines.append(_shell_code(line[: quote_offsets[0]]))
        else:
            code_lines.append(_shell_code(line))
    return code_lines


def _shell_function_name(line: str) -> str | None:
    function = SHELL_FUNCTION.fullmatch(line)
    if function is None:
        return None
    return function.group(1) or function.group(2)


def _function_scopes(lines: list[str]) -> list[str | None]:
    scopes: list[str | None] = []
    function_scope: str | None = None
    group_depth = 0
    for line in lines:
        function_name = _shell_function_name(line)
        if function_name is not None:
            function_scope = function_name
            group_depth = 0
            scopes.append(None)
            continue
        stripped = line.strip()
        if function_scope is not None and re.search(r"(?:\|\||&&)\s*\{$", stripped):
            group_depth += 1
        elif function_scope is not None and stripped == "}":
            if group_depth:
                group_depth -= 1
            else:
                function_scope = None
                scopes.append(None)
                continue
        scopes.append(function_scope)
    return scopes


def _scope_lines(lines: list[str], scope: str) -> list[str]:
    return [
        line
        for line, function_scope in zip(
            lines, _function_scopes(lines), strict=True
        )
        if function_scope == scope
    ]


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
