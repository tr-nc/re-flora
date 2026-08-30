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
TOOL_FUNCTION_DEFINITION = re.compile(
    r"^\s*(?:function\s+)?(command|cargo|tee|python3)"
    r"(?:\s*\(\s*\))?\s*\{"
)
CANONICAL_CARGO_BUILD = re.compile(
    r'^\s*/usr/bin/env cargo build (?:--quiet )?--release --manifest-path "\$repo_root/Cargo\.toml"\s*$'
)
CANONICAL_CARGO_RUN = re.compile(
    r'^\s*/usr/bin/env cargo run --quiet --release --manifest-path "\$repo_root/Cargo\.toml" --\s*$'
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
                if _references_parameter(stripped, "dry_run"):
                    frames[-1]["dry_run"] = True
        elif re.match(r"^else(?:\s*;|\s*$)", stripped):
            pass
        elif re.match(r"^if\b", stripped):
            frames.append(
                {
                    "dry_run": _references_parameter(stripped, "dry_run"),
                    "in_condition": not _has_then_token(stripped),
                }
            )
        elif frames and bool(frames[-1]["in_condition"]):
            if _references_parameter(stripped, "dry_run"):
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
    _, active_lines, _ = _shell_lex_lines(lines)

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

    canonical_dry_lines = {
        "dry_run=false",
        "dry_run=true",
        "readonly dry_run",
    }
    for line_number, (line, active) in enumerate(
        zip(lines, active_lines, strict=True), 1
    ):
        for authority in _authority_operations(active):
            if authority == "dry_run" and line.strip() not in canonical_dry_lines:
                authority_failures.append(f"dry_run:{line_number}")
                unknown_dry_run.append(str(line_number))
            elif authority == "repo_root" and line.strip() != CANONICAL_REPO_ROOT:
                authority_failures.append(f"repo_root:{line_number}")

    code_lines, active_lines, _ = _shell_lex_lines(lines)
    authority_failures.extend(
        f"dynamic:{line_number}:{reason}"
        for line_number, reason in _forbidden_dynamic_authority(
            code_lines, active_lines
        )
    )

    return authority_failures, unknown_dry_run


def _authority_operations(active: str) -> tuple[str, ...]:
    operations: list[str] = []
    assignment = re.compile(
        r"(?<![$A-Za-z0-9_])(?P<base>dry_run|repo_root)"
        r"(?:\[[^]\n]*\])?(?:\+=|=)"
    )
    operations.extend(match.group("base") for match in assignment.finditer(active))
    command_mutation = re.compile(
        r"\b(?:readonly|unset)(?:\s+-[A-Za-z]+)*\s+"
        r"(?P<base>dry_run|repo_root)\b"
    )
    operations.extend(
        match.group("base") for match in command_mutation.finditer(active)
    )
    for start, end, base in _active_parameter_expansions(active):
        if _parameter_expansion_assigns(active[start:end], base):
            operations.append(base)
    return tuple(operations)


def _parameter_expansion_assigns(expansion: str, base: str) -> bool:
    if not expansion.startswith("${"):
        return False
    content = expansion[2:-1]
    if content.startswith(("#", "!")):
        content = content[1:]
    if not content.startswith(base):
        return False
    suffix = content[len(base) :]
    if suffix.startswith("["):
        closing = suffix.find("]")
        if closing < 0:
            return False
        suffix = suffix[closing + 1 :]
    return suffix.startswith(("=", ":="))


def _forbidden_dynamic_authority(
    code_lines: list[str], active_lines: list[str]
) -> tuple[tuple[int, str], ...]:
    failures: list[tuple[int, str]] = []
    for line_number, code, active in _logical_shell_lines(
        code_lines, active_lines
    ):
        if not active.strip() and not code.lstrip().startswith(("'", '"')):
            continue
        for command in _shell_commands(code):
            executable, arguments = _resolved_shell_command(command)
            if executable in {"eval", "source", "."}:
                failures.append((line_number, executable))
            elif executable == "printf" and _printf_writes_authority(arguments):
                failures.append((line_number, "printf-v"))
            elif executable == "read" and _read_writes_authority(arguments):
                failures.append((line_number, "read"))
            elif executable in {"readarray", "mapfile"} and _array_reader_writes_authority(
                arguments
            ):
                failures.append((line_number, executable))
            elif executable == "getopts" and _getopts_writes_authority(arguments):
                failures.append((line_number, "getopts"))
            elif executable == "let" and _let_writes_authority(arguments):
                failures.append((line_number, "let"))
            elif executable in {"declare", "typeset", "local"} and _has_nameref_option(
                arguments
            ):
                failures.append((line_number, "nameref"))
            elif (
                executable.rsplit("/", 1)[-1] in {"bash", "sh", "zsh"}
                and "-c" in arguments
            ):
                failures.append((line_number, "shell-c"))
    return tuple(failures)


def _logical_shell_lines(
    code_lines: list[str], active_lines: list[str]
) -> tuple[tuple[int, str, str], ...]:
    logical: list[tuple[int, str, str]] = []
    start_line = 1
    code_parts: list[str] = []
    active_parts: list[str] = []
    for line_number, (code, active) in enumerate(
        zip(code_lines, active_lines, strict=True), 1
    ):
        if not code_parts:
            start_line = line_number
        continued = code.rstrip().endswith("\\")
        code_parts.append(code.rstrip()[:-1] if continued else code)
        active_parts.append(active.rstrip()[:-1] if continued else active)
        if continued:
            continue
        logical.append(
            (start_line, " ".join(code_parts), " ".join(active_parts))
        )
        code_parts.clear()
        active_parts.clear()
    if code_parts:
        logical.append((start_line, " ".join(code_parts), " ".join(active_parts)))
    return tuple(logical)


def _shell_commands(code: str) -> tuple[tuple[str, ...], ...]:
    commands: list[tuple[str, ...]] = []
    segment: list[str] = []
    for token in (*_shell_tokens(code), ";"):
        if token not in {";", "|", "||", "&", "&&"}:
            segment.append(token)
            continue
        while segment and segment[0] in {
            "if",
            "elif",
            "then",
            "else",
            "do",
            "!",
            "(",
        }:
            segment.pop(0)
        while segment and _is_shell_assignment_word(segment[0]):
            segment.pop(0)
        if segment and segment[0] not in {")", "fi", "done", "{", "}"}:
            commands.append(tuple(segment))
        segment.clear()
    return tuple(commands)


def _is_shell_assignment_word(token: str) -> bool:
    return (
        re.fullmatch(
            r"[A-Za-z_][A-Za-z0-9_]*(?:\[[^]]*\])?(?:\+=|=).*",
            token,
        )
        is not None
    )


def _resolved_shell_command(command: tuple[str, ...]) -> tuple[str, tuple[str, ...]]:
    words = list(command)
    while words and words[0] in {"builtin", "command"}:
        words.pop(0)
        while words and words[0].startswith("-"):
            words.pop(0)
    if words and words[0] == "/usr/bin/env":
        words.pop(0)
        while words and (words[0].startswith("-") or _is_shell_assignment_word(words[0])):
            words.pop(0)
    if not words:
        return "", ()
    return words[0], tuple(words[1:])


def _authority_or_dynamic_target(target: str) -> bool:
    base = target.split("[", 1)[0]
    return base in {"dry_run", "repo_root"} or any(
        marker in target for marker in ("$", "`", "$(")
    )


def _printf_writes_authority(arguments: tuple[str, ...]) -> bool:
    for index, argument in enumerate(arguments):
        if argument == "-v":
            return index + 1 >= len(arguments) or _authority_or_dynamic_target(
                arguments[index + 1]
            )
        if argument.startswith("-v") and len(argument) > 2:
            return _authority_or_dynamic_target(argument[2:])
    return False


def _read_writes_authority(arguments: tuple[str, ...]) -> bool:
    targets: list[str] = []
    index = 0
    option_arguments = ("a", "d", "i", "n", "N", "p", "t", "u")
    while index < len(arguments):
        argument = arguments[index]
        if argument == "--":
            index += 1
            break
        if not argument.startswith("-") or argument == "-":
            break
        option = argument[1:]
        consuming = next((flag for flag in option_arguments if flag in option), None)
        if consuming == "a":
            if option.endswith("a"):
                index += 1
                if index >= len(arguments):
                    return True
                targets.append(arguments[index])
            else:
                targets.append(option.split("a", 1)[1])
        elif consuming is not None and option.endswith(consuming):
            index += 1
        index += 1
    targets.extend(
        argument
        for argument in arguments[index:]
        if not argument.startswith(("<", ">"))
    )
    return any(_authority_or_dynamic_target(target) for target in targets)


def _array_reader_writes_authority(arguments: tuple[str, ...]) -> bool:
    index = 0
    consuming_options = {"-n", "-O", "-s", "-C", "-c", "-u"}
    while index < len(arguments):
        argument = arguments[index]
        if argument == "--":
            index += 1
            break
        if argument in consuming_options:
            index += 2
            continue
        if argument.startswith("-"):
            index += 1
            continue
        break
    targets = [
        argument
        for argument in arguments[index:]
        if not argument.startswith(("<", ">"))
    ]
    return any(_authority_or_dynamic_target(target) for target in targets)


def _getopts_writes_authority(arguments: tuple[str, ...]) -> bool:
    return len(arguments) < 2 or _authority_or_dynamic_target(arguments[1])


def _let_writes_authority(arguments: tuple[str, ...]) -> bool:
    mutation = re.compile(
        r"(?:^|[^$A-Za-z0-9_])(dry_run|repo_root)\s*"
        r"(?:\+\+|--|(?:<<|>>|[+\-*/%&^|])?=(?!=))"
        r"|(?:\+\+|--)\s*(dry_run|repo_root)\b"
    )
    return any(
        "$" in argument or "`" in argument or mutation.search(argument) is not None
        for argument in arguments
    )


def _has_nameref_option(arguments: tuple[str, ...]) -> bool:
    for argument in arguments:
        if argument == "--":
            return False
        if re.fullmatch(r"[-+][A-Za-z]+", argument) is None:
            return False
        if "n" in argument[1:]:
            return True
    return False


def _references_parameter(line: str, expected_base: str) -> bool:
    return any(
        base == expected_base
        for _, _, base in _active_parameter_expansions(line)
    )


def _active_parameter_expansions(
    line: str,
) -> tuple[tuple[int, int, str], ...]:
    expansions: list[tuple[int, int, str]] = []
    quote: str | None = None
    index = 0
    while index < len(line):
        character = line[index]
        if character == "\\" and quote != "'":
            index += 2
            continue
        if quote == "'":
            if character == "'":
                quote = None
            index += 1
            continue
        if character == '"':
            quote = None if quote == '"' else '"'
            index += 1
            continue
        if quote is None and character == "'":
            quote = "'"
            index += 1
            continue
        if quote is None and character == "#" and _starts_shell_comment(line, index):
            break
        if character != "$":
            index += 1
            continue
        parsed = _parameter_expansion_at(line, index)
        if parsed is None:
            index += 1
            continue
        end, base = parsed
        if base is not None:
            expansions.append((index, end, base))
        index = end
    return tuple(expansions)


def _parameter_expansion_at(
    line: str, start: int
) -> tuple[int, str | None] | None:
    if start + 1 >= len(line):
        return None
    if line[start + 1] == "{":
        end = _braced_parameter_end(line, start + 1)
        if end is None:
            return None
        content = line[start + 2 : end - 1]
        if content.startswith(("#", "!")):
            content = content[1:]
        base = re.match(r"[A-Za-z_][A-Za-z0-9_]*", content)
        return end, base.group(0) if base is not None else None
    base = re.match(r"[A-Za-z_][A-Za-z0-9_]*", line[start + 1 :])
    if base is None:
        return None
    return start + 1 + base.end(), base.group(0)


def _braced_parameter_end(line: str, opening_brace: int) -> int | None:
    depth = 1
    index = opening_brace + 1
    while index < len(line):
        if line[index] == "\\":
            index += 2
            continue
        if line.startswith("${", index):
            depth += 1
            index += 2
            continue
        if line[index] == "}":
            depth -= 1
            if depth == 0:
                return index + 1
        index += 1
    return None


def _shell_active_text(line: str) -> str:
    return _shell_lex_lines([line])[1][0]


def _transport_sink_policy_is_sealed(lines: list[str]) -> bool:
    statements = [
        _shell_code(line).strip()
        for line in _scope_lines(lines, "execute_analysis")
        if _shell_code(line).strip()
    ]
    dry_sink = re.compile(r"local\s+sink=\(\s*cat\s*\)")
    production_sink = re.compile(
        r'sink=\(\s*/usr/bin/env\s+tee\s+"\$json"\s*\)'
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
    code_lines, active_lines, _ = _shell_lex_lines(lines)
    for line_number, (line, code, active) in enumerate(
        zip(lines, code_lines, active_lines, strict=True), 1
    ):
        cargo_occurrences = tuple(CARGO_IDENTIFIER.finditer(active))
        if cargo_occurrences:
            if len(cargo_occurrences) != 1:
                failures.append(f"cargo:{line_number}")
            elif CANONICAL_CARGO_BUILD.fullmatch(code):
                cargo_builds += 1
                if not _cargo_build_is_non_dry_run(lines, line_number - 1):
                    failures.append(f"cargo-build-policy:{line_number}")
            elif CANONICAL_CARGO_RUN.fullmatch(code):
                cargo_runs += 1
            else:
                failures.append(f"cargo:{line_number}")
        function_name = _shell_function_name(line)
        if (
            function_name in {"command", "cargo", "tee", "python3"}
            or TOOL_FUNCTION_DEFINITION.match(active)
            or re.match(
                r"^\s*alias\s+(?:command|cargo|tee|python3)\s*=", active
            )
        ):
            failures.append(f"tool-shadow:{line_number}")
        tee_occurrences = tuple(re.finditer(r"\btee\b", active))
        if tee_occurrences and not (
            len(tee_occurrences) == 1
            and re.search(r"/usr/bin/env\s+tee\b", active) is not None
        ):
            failures.append(f"tee:{line_number}")
        python_occurrences = tuple(re.finditer(r"\bpython3\b", active))
        if python_occurrences and not (
            len(python_occurrences) == 1
            and re.search(r"/usr/bin/env\s+python3\b", active) is not None
        ):
            failures.append(f"python3:{line_number}")
        if APP_IDENTIFIER.search(active) is not None:
            failures.append(f"re-flora:{line_number}")
        command_words = (
            _shell_command_words(code)
            if active.strip() or code.lstrip().startswith(("'", '"'))
            else ()
        )
        for executable in command_words:
            basename = executable.rsplit("/", 1)[-1]
            if basename in {"cargo", "re-flora"}:
                failures.append(f"command:{line_number}")
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
    for reverse_index, statement in enumerate(reversed(significant)):
        policy = _dry_run_condition_policy(statement)
        if policy == "non-dry":
            return True
        if statement == "fi" or policy == "dry":
            return False
        if statement == "else":
            earlier = significant[: len(significant) - reverse_index - 1]
            return any(
                _dry_run_condition_policy(candidate) == "dry"
                for candidate in reversed(earlier)
            )
    return False


def _command_array_launch_is_non_dry_run(
    lines: list[str], line_index: int
) -> bool:
    if _non_dry_run_branch_lines(lines)[line_index]:
        return True
    scopes = _function_scopes(lines)
    scope = scopes[line_index]
    start = max(0, line_index - 18)
    scoped = [
        _shell_code(lines[index]).strip()
        for index in range(start, line_index)
        if scopes[index] == scope
    ]
    return _has_completed_dry_run_exit_guard(scoped)


def _has_completed_dry_run_exit_guard(statements: list[str]) -> bool:
    for start, statement in enumerate(statements):
        if _dry_run_condition_policy(statement) != "dry":
            continue
        depth = 0
        exits = False
        for nested in statements[start:]:
            if re.match(r"^if\b", nested):
                depth += 1
            if depth == 1 and re.search(r"\b(?:return\s+0|continue)\b", nested):
                exits = True
            if re.fullmatch(r"fi\s*;?", nested):
                depth -= 1
                if depth == 0:
                    return exits
    return False


def _dry_run_condition_policy(statement: str) -> str:
    if_header = re.match(r"^if\b", statement)
    if if_header is None:
        return "unknown"
    expansions = [
        expansion
        for expansion in _active_parameter_expansions(statement)
        if expansion[2] == "dry_run"
    ]
    if len(expansions) != 1:
        return "unknown"
    prefix = statement[if_header.end() : expansions[0][0]].strip()
    if prefix == "":
        return "dry"
    if prefix == "!":
        return "non-dry"
    return "unknown"


def _non_dry_run_branch_lines(lines: list[str]) -> list[bool]:
    policies: list[str] = []
    result: list[bool] = []
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
            policy = _dry_run_condition_policy(statement)
            if policy == "non-dry":
                policies.append("non-dry")
            elif policy == "dry":
                policies.append("dry")
            else:
                policies.append("unknown")
        result.append(any(policy == "non-dry" for policy in policies))
    return result


def _has_then_token(statement: str) -> bool:
    return re.search(r"(?:^|[;\s])then(?:$|[;\s])", statement) is not None


def _shell_code(line: str) -> str:
    return _shell_lex_lines([line])[0][0]


def _shell_code_lines(lines: list[str]) -> list[str]:
    """Return structural code with multiline quoted program bodies masked."""
    return _shell_lex_lines(lines)[2]


def _shell_lex_lines(
    lines: list[str],
) -> tuple[list[str], list[str], list[str]]:
    """Lex the controlled runner grammar into code, active, and structural text."""
    code_lines: list[str] = []
    active_lines: list[str] = []
    structural_lines: list[str] = []
    quote: str | None = None
    command_substitutions: list[dict[str, object]] = []
    for line in lines:
        code = [" "] * len(line)
        active = [" "] * len(line)
        structural = [" "] * len(line)
        index = 0
        while index < len(line):
            character = line[index]
            if quote == "'":
                code[index] = character
                if character == "'":
                    quote = None
                index += 1
                continue
            if quote == '"':
                code[index] = character
                structural[index] = character
                if character == "\\" and index + 1 < len(line):
                    code[index + 1] = line[index + 1]
                    structural[index + 1] = line[index + 1]
                    index += 2
                    continue
                if character == '"':
                    quote = None
                    index += 1
                    continue
                if line.startswith("$(", index):
                    code[index : index + 2] = line[index : index + 2]
                    structural[index : index + 2] = line[index : index + 2]
                    active[index : index + 2] = line[index : index + 2]
                    command_substitutions.append(
                        {"resume_quote": '"', "depth": 1}
                    )
                    quote = None
                    index += 2
                    continue
                if character == "$":
                    parsed = _parameter_expansion_at(line, index)
                    if parsed is not None:
                        end, _ = parsed
                        active[index:end] = line[index:end]
                        code[index:end] = line[index:end]
                        structural[index:end] = line[index:end]
                        index = end
                        continue
                index += 1
                continue
            if character == "#" and _starts_shell_comment(line, index):
                break
            code[index] = character
            active[index] = character
            structural[index] = character
            if line.startswith("$(", index):
                code[index : index + 2] = line[index : index + 2]
                active[index : index + 2] = line[index : index + 2]
                structural[index : index + 2] = line[index : index + 2]
                command_substitutions.append(
                    {"resume_quote": None, "depth": 1}
                )
                index += 2
                continue
            if command_substitutions and character == "(":
                command_substitutions[-1]["depth"] = (
                    int(command_substitutions[-1]["depth"]) + 1
                )
            elif command_substitutions and character == ")":
                command_substitutions[-1]["depth"] = (
                    int(command_substitutions[-1]["depth"]) - 1
                )
                if command_substitutions[-1]["depth"] == 0:
                    resume_quote = command_substitutions.pop()["resume_quote"]
                    quote = resume_quote if isinstance(resume_quote, str) else None
            if character == "\\" and index + 1 < len(line):
                code[index + 1] = line[index + 1]
                structural[index + 1] = line[index + 1]
                active[index] = " "
                active[index + 1] = " "
                index += 2
                continue
            if character == "'":
                quote = "'"
                active[index] = " "
                structural[index] = " "
            elif character == '"':
                quote = '"'
                active[index] = " "
            index += 1
        code_lines.append("".join(code))
        active_lines.append("".join(active))
        structural_lines.append("".join(structural))
    return code_lines, active_lines, structural_lines


def _starts_shell_comment(line: str, index: int) -> bool:
    return index == 0 or line[index - 1].isspace() or line[index - 1] in ";|&()"


def _shell_command_words(code: str) -> tuple[str, ...]:
    tokens = _shell_tokens(code)
    commands: list[str] = []
    expect_command = True
    for token in tokens:
        if token in {";", "|", "||", "&", "&&"}:
            expect_command = True
            continue
        if token in {"if", "elif", "then", "else", "do", "!", "("}:
            expect_command = True
            continue
        if token in {")", "fi", "done", "{", "}"}:
            continue
        if expect_command:
            if re.fullmatch(
                r"[A-Za-z_][A-Za-z0-9_]*(?:\[[^]]*\])?(?:\+=|=).+",
                token,
            ):
                continue
            commands.append(token)
            expect_command = False
    return tuple(commands)


def _shell_tokens(code: str) -> tuple[str, ...]:
    tokens: list[str] = []
    current: list[str] = []
    quote: str | None = None
    index = 0

    def finish_word() -> None:
        if current:
            tokens.append("".join(current))
            current.clear()

    while index < len(code):
        character = code[index]
        if quote is not None:
            if character == quote:
                quote = None
            elif character == "\\" and quote == '"' and index + 1 < len(code):
                current.append(code[index + 1])
                index += 1
            else:
                current.append(character)
            index += 1
            continue
        if character in "'\"":
            quote = character
            index += 1
            continue
        if character == "\\" and index + 1 < len(code):
            current.append(code[index + 1])
            index += 2
            continue
        if character.isspace():
            finish_word()
            index += 1
            continue
        if character in ";|&()":
            finish_word()
            if character in "|&" and index + 1 < len(code) and code[index + 1] == character:
                tokens.append(character * 2)
                index += 2
            else:
                tokens.append(character)
                index += 1
            continue
        current.append(character)
        index += 1
    finish_word()
    return tuple(tokens)


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
