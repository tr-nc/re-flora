#!/usr/bin/env python3
from __future__ import annotations

import sys
from pathlib import Path
from typing import NamedTuple


OWNER = "src/app/core/lighting_mode_acceptance.rs"
CALLER = "src/app/core/mod.rs"
TRACER_OWNER = "src/tracer/mod.rs"
BUFFER_UPDATER_OWNER = "src/tracer/buffer_updater.rs"
ENVIRONMENT_OWNER = "src/environment_lighting.rs"
RESOLVED_TYPES = ("ResolvedLightingFrameInputs", "ResolvedFrameTiming")
PLAN_METHODS = ("frame_plan", "resolve_timing", "resolve_lighting")


class Function(NamedTuple):
    name: str
    generic_tokens: tuple[str, ...]
    parameters: tuple[tuple[str, ...], ...]
    body_start: int | None
    body_end: int | None


def read_sources(src_root: Path) -> dict[str, str]:
    repo_root = src_root.parent
    return {
        path.relative_to(repo_root).as_posix(): path.read_text()
        for path in sorted(src_root.rglob("*.rs"))
    }


def _raw_string_end(source: str, index: int) -> int | None:
    for prefix in ("br", "cr", "r"):
        if not source.startswith(prefix, index):
            continue
        quote = index + len(prefix)
        while quote < len(source) and source[quote] == "#":
            quote += 1
        if quote >= len(source) or source[quote] != '"':
            continue
        terminator = '"' + ("#" * (quote - index - len(prefix)))
        end = source.find(terminator, quote + 1)
        return len(source) if end < 0 else end + len(terminator)
    return None


def _quoted_end(source: str, quote: int, delimiter: str) -> int:
    index = quote + 1
    while index < len(source):
        if source[index] == "\\":
            index += 2
        elif source[index] == delimiter:
            return index + 1
        else:
            index += 1
    return len(source)


def tokenize(source: str) -> list[str]:
    """Tokenize the Rust syntax needed by this audit, excluding all literal contents."""
    tokens: list[str] = []
    index = 0
    length = len(source)
    while index < length:
        char = source[index]
        if char.isspace():
            index += 1
            continue
        if source.startswith("//", index):
            newline = source.find("\n", index + 2)
            index = length if newline < 0 else newline + 1
            continue
        if source.startswith("/*", index):
            depth = 1
            index += 2
            while index < length and depth:
                if source.startswith("/*", index):
                    depth += 1
                    index += 2
                elif source.startswith("*/", index):
                    depth -= 1
                    index += 2
                else:
                    index += 1
            continue

        raw_end = _raw_string_end(source, index)
        if raw_end is not None:
            index = raw_end
            continue

        if char in ("b", "c") and index + 1 < length and source[index + 1] == '"':
            index = _quoted_end(source, index + 1, '"')
            continue
        if char == '"':
            index = _quoted_end(source, index, '"')
            continue
        if char == "b" and index + 1 < length and source[index + 1] == "'":
            index = _quoted_end(source, index + 1, "'")
            continue
        if char == "'":
            # A closing quote distinguishes a character from a lifetime such as 'frame.
            if index + 2 < length and source[index + 2] == "'":
                tokens.append("<char>")
                index += 3
                continue
            if index + 1 < length and source[index + 1] == "\\":
                char_end = _quoted_end(source, index, "'")
                tokens.append("<char>")
                index = char_end
                continue
            tokens.append(char)
            index += 1
            continue

        if source.startswith("r#", index) and index + 2 < length:
            first = source[index + 2]
            if first == "_" or first.isalpha():
                end = index + 3
                while end < length and (source[end] == "_" or source[end].isalnum()):
                    end += 1
                tokens.append(source[index + 2 : end])
                index = end
                continue
        if char == "_" or char.isalpha():
            end = index + 1
            while end < length and (source[end] == "_" or source[end].isalnum()):
                end += 1
            tokens.append(source[index:end])
            index = end
            continue
        if char.isdigit():
            end = index + 1
            while end < length and (source[end] == "_" or source[end].isalnum()):
                end += 1
            tokens.append(source[index:end])
            index = end
            continue
        tokens.append(char)
        index += 1
    return tokens


def _closing(tokens: list[str], opening: int, left: str, right: str) -> int | None:
    depth = 0
    for index in range(opening, len(tokens)):
        if tokens[index] == left:
            depth += 1
        elif tokens[index] == right:
            depth -= 1
            if depth == 0:
                return index
    return None


def _contains(tokens: list[str] | tuple[str, ...], sequence: tuple[str, ...]) -> bool:
    width = len(sequence)
    return any(tuple(tokens[index : index + width]) == sequence for index in range(len(tokens)))


def _struct_body(tokens: list[str], name: str) -> list[str] | None:
    for index in range(len(tokens) - 2):
        if tokens[index : index + 2] == ["struct", name]:
            try:
                opening = tokens.index("{", index + 2)
            except ValueError:
                return None
            closing = _closing(tokens, opening, "{", "}")
            return None if closing is None else tokens[opening + 1 : closing]
    return None


def _split_top_level(tokens: list[str], delimiter: str) -> list[tuple[str, ...]]:
    chunks: list[tuple[str, ...]] = []
    start = 0
    depths = {"(": 0, "[": 0, "{": 0, "<": 0}
    pairs = {")": "(", "]": "[", "}": "{", ">": "<"}
    for index, token in enumerate(tokens):
        if token in depths:
            depths[token] += 1
        elif token in pairs and depths[pairs[token]]:
            depths[pairs[token]] -= 1
        elif token == delimiter and not any(depths.values()):
            if index > start:
                chunks.append(tuple(tokens[start:index]))
            start = index + 1
    if start < len(tokens):
        chunks.append(tuple(tokens[start:]))
    return chunks


def _function_at(tokens: list[str], fn_index: int) -> Function | None:
    if fn_index + 1 >= len(tokens):
        return None
    name = tokens[fn_index + 1]
    cursor = fn_index + 2
    generic_tokens: tuple[str, ...] = ()
    if cursor < len(tokens) and tokens[cursor] == "<":
        closing = _closing(tokens, cursor, "<", ">")
        if closing is None:
            return None
        generic_tokens = tuple(tokens[cursor + 1 : closing])
        cursor = closing + 1
    if cursor >= len(tokens) or tokens[cursor] != "(":
        return None
    params_end = _closing(tokens, cursor, "(", ")")
    if params_end is None:
        return None
    parameters = tuple(_split_top_level(tokens[cursor + 1 : params_end], ","))

    body_start = None
    body_end = None
    cursor = params_end + 1
    angle_depth = 0
    while cursor < len(tokens):
        token = tokens[cursor]
        if token == "<":
            angle_depth += 1
        elif token == ">" and angle_depth:
            angle_depth -= 1
        elif not angle_depth and token == ";":
            break
        elif not angle_depth and token == "{":
            body_start = cursor
            body_end = _closing(tokens, cursor, "{", "}")
            break
        cursor += 1
    return Function(name, generic_tokens, parameters, body_start, body_end)


def _inherent_impl_functions(tokens: list[str], type_name: str) -> list[Function]:
    functions: list[Function] = []
    for impl_index, token in enumerate(tokens):
        if token != "impl":
            continue
        cursor = impl_index + 1
        if cursor < len(tokens) and tokens[cursor] == "<":
            closing = _closing(tokens, cursor, "<", ">")
            if closing is None:
                continue
            cursor = closing + 1
        header_start = cursor
        angle_depth = 0
        while cursor < len(tokens):
            if tokens[cursor] == "<":
                angle_depth += 1
            elif tokens[cursor] == ">" and angle_depth:
                angle_depth -= 1
            elif tokens[cursor] == "{" and not angle_depth:
                break
            cursor += 1
        if cursor >= len(tokens):
            continue
        header = tokens[header_start:cursor]
        if "for" in header or not header or header[0] != type_name:
            continue
        impl_end = _closing(tokens, cursor, "{", "}")
        if impl_end is None:
            continue
        depth = 0
        item = cursor + 1
        while item < impl_end:
            if tokens[item] == "{":
                depth += 1
            elif tokens[item] == "}":
                depth -= 1
            elif tokens[item] == "fn" and depth == 0:
                function = _function_at(tokens, item)
                if function is not None:
                    functions.append(function)
                    if function.body_end is not None:
                        item = function.body_end
            item += 1
    return functions


def _direct_named_parameter(
    function: Function, expected_type: tuple[str, ...]
) -> str | None:
    for parameter in function.parameters:
        if ":" not in parameter:
            continue
        colon = parameter.index(":")
        if colon == 1 and tuple(parameter[colon + 1 :]) == expected_type:
            return parameter[0]
    return None


def _method_reference_count(tokens: list[str], name: str) -> int:
    count = 0
    for index, token in enumerate(tokens):
        if token != name:
            continue
        if index > 0 and tokens[index - 1] == "fn":
            continue
        dotted = index > 0 and tokens[index - 1] == "."
        associated = index > 1 and tokens[index - 2 : index] == [":", ":"]
        if dotted or associated:
            count += 1
    return count


def _call_arguments(tokens: list[str], prefix: tuple[str, ...]) -> list[list[tuple[str, ...]]]:
    calls: list[list[tuple[str, ...]]] = []
    for index in range(len(tokens) - len(prefix) + 1):
        if tuple(tokens[index : index + len(prefix)]) != prefix:
            continue
        opening = index + len(prefix) - 1
        closing = _closing(tokens, opening, "(", ")")
        if closing is not None:
            calls.append(_split_top_level(tokens[opening + 1 : closing], ","))
    return calls


def _canonical_function(
    tokens: list[str], impl_name: str, function_name: str
) -> Function | None:
    matches = [
        function
        for function in _inherent_impl_functions(tokens, impl_name)
        if function.name == function_name
    ]
    return matches[0] if len(matches) == 1 else None


def audit(sources: dict[str, str]) -> list[str]:
    errors: list[str] = []
    tokenized = {path: tokenize(source) for path, source in sources.items()}
    owner = tokenized.get(OWNER)
    if owner is None:
        return [f"missing owner: {OWNER}"]
    external = {path: tokens for path, tokens in tokenized.items() if path != OWNER}

    for type_name in RESOLVED_TYPES:
        body = _struct_body(owner, type_name)
        if body is None:
            errors.append(f"owner missing struct {type_name}")
        elif "pub" in body:
            errors.append(f"{type_name} exposes field visibility")

    for type_name in RESOLVED_TYPES:
        for path, tokens in external.items():
            if _contains(tokens, (type_name, "{")):
                errors.append(f"external construction/destructure of {type_name}: {path}")

    for method in PLAN_METHODS:
        sites = [
            path
            for path, tokens in external.items()
            for _ in range(_method_reference_count(tokens, method))
        ]
        if sites != [CALLER]:
            errors.append(f"{method} call sites must be exactly [{CALLER}], got {sites}")

    tracer_tokens = tokenized.get(TRACER_OWNER, [])
    update_buffers = _canonical_function(tracer_tokens, "Tracer", "update_buffers")
    if update_buffers is None:
        errors.append("Tracer inherent impl must own exactly one update_buffers entry")
    else:
        if update_buffers.generic_tokens:
            errors.append("Tracer::update_buffers must use the production non-generic entry")
        if ("&", "mut", "self") not in update_buffers.parameters:
            errors.append("Tracer::update_buffers requires &mut self receiver")
        lighting_parameter = _direct_named_parameter(
            update_buffers, ("&", "ResolvedLightingFrameInputs")
        )
        if lighting_parameter is None:
            errors.append(
                "Tracer::update_buffers requires a direct &ResolvedLightingFrameInputs parameter"
            )
        elif update_buffers.body_start is not None and update_buffers.body_end is not None:
            body = tracer_tokens[update_buffers.body_start + 1 : update_buffers.body_end]
            if not _contains(
                body,
                (
                    "let",
                    "raster_lighting_mode",
                    "=",
                    lighting_parameter,
                    ".",
                    "raster_lighting_mode",
                    "(",
                    ")",
                    ";",
                ),
            ):
                errors.append("Tracer::update_buffers must resolve the typed raster lighting mode")
            updater_calls = _call_arguments(
                body, ("BufferUpdater", ":", ":", "update_gui_input", "(")
            )
            if len(updater_calls) != 1 or ("raster_lighting_mode",) not in updater_calls[0]:
                errors.append("Tracer::update_buffers must route the typed mode to BufferUpdater")

    updater_tokens = tokenized.get(BUFFER_UPDATER_OWNER, [])
    update_gui_input = _canonical_function(updater_tokens, "BufferUpdater", "update_gui_input")
    if update_gui_input is None:
        errors.append("BufferUpdater inherent impl must own exactly one update_gui_input entry")
    else:
        if update_gui_input.generic_tokens:
            errors.append("BufferUpdater::update_gui_input must use the production non-generic entry")
        if any("self" in parameter for parameter in update_gui_input.parameters):
            errors.append("BufferUpdater::update_gui_input must remain an associated function")
        raster_parameter = _direct_named_parameter(update_gui_input, ("RasterLightingMode",))
        if raster_parameter is None:
            errors.append(
                "BufferUpdater::update_gui_input requires a direct RasterLightingMode parameter"
            )
        elif update_gui_input.body_start is not None and update_gui_input.body_end is not None:
            body = updater_tokens[update_gui_input.body_start + 1 : update_gui_input.body_end]
            if not _contains(
                body,
                (
                    "raster_flora_ddgi_lighting",
                    ":",
                    raster_parameter,
                    ".",
                    "is_ddgi",
                    "(",
                    ")",
                    "as",
                    "u32",
                ),
            ):
                errors.append("GUI uniform mode must derive from the typed RasterLightingMode")

    sink_sites: list[tuple[str, int]] = []
    for path, tokens in tokenized.items():
        for index in range(len(tokens) - 3):
            if tokens[index : index + 4] == ["gui_input", ".", "fill_uniform", "("]:
                sink_sites.append((path, index))
    if len(sink_sites) != 1:
        errors.append(f"gui_input.fill_uniform sink must be globally unique, got {sink_sites}")
    elif update_gui_input is None or sink_sites[0][0] != BUFFER_UPDATER_OWNER:
        errors.append("gui_input.fill_uniform sink must belong to BufferUpdater::update_gui_input")
    elif not (
        update_gui_input.body_start is not None
        and update_gui_input.body_end is not None
        and update_gui_input.body_start < sink_sites[0][1] < update_gui_input.body_end
    ):
        errors.append("gui_input.fill_uniform sink must be inside BufferUpdater::update_gui_input")

    if "ResolvedLightingFrameInputs" in tokenized.get(ENVIRONMENT_OWNER, []):
        errors.append(f"acceptance resolved input leaked into {ENVIRONMENT_OWNER}")
    return errors


def main() -> int:
    repo_root = Path(__file__).resolve().parents[1]
    errors = audit(read_sources(repo_root / "src"))
    if errors:
        for error in errors:
            print(f"lighting-mode acceptance source contract: {error}", file=sys.stderr)
        return 1
    print("lighting-mode acceptance source contract: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
