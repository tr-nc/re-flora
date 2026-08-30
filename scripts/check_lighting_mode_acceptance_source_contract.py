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
RESOLVED_TYPES = (
    "ResolvedLightingFrameInputs",
    "ResolvedFrameTiming",
    "ResolvedRasterLightingState",
)


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


def _module_root_struct(tokens: list[str], name: str) -> tuple[int, list[str]] | None:
    matches: list[tuple[int, list[str]]] = []
    brace_depth = 0
    for index, token in enumerate(tokens[:-1]):
        if brace_depth == 0 and tokens[index : index + 2] == ["struct", name]:
            try:
                opening = tokens.index("{", index + 2)
            except ValueError:
                continue
            closing = _closing(tokens, opening, "{", "}")
            if closing is not None:
                matches.append((index, tokens[opening + 1 : closing]))
        if token == "{":
            brace_depth += 1
        elif token == "}" and brace_depth:
            brace_depth -= 1
    return matches[0] if len(matches) == 1 else None


def _struct_body(tokens: list[str], name: str) -> list[str] | None:
    root_struct = _module_root_struct(tokens, name)
    return None if root_struct is None else root_struct[1]


def _module_root_function(
    tokens: list[str], name: str
) -> tuple[int, Function] | None:
    matches: list[tuple[int, Function]] = []
    brace_depth = 0
    for index, token in enumerate(tokens):
        if brace_depth == 0 and token == "fn" and index + 1 < len(tokens):
            function = _function_at(tokens, index)
            if function is not None and function.name == name:
                matches.append((index, function))
        if token == "{":
            brace_depth += 1
        elif token == "}" and brace_depth:
            brace_depth -= 1
    return matches[0] if len(matches) == 1 else None


def _struct_expression_count(tokens: list[str], type_name: str) -> int:
    function_body_openings = {
        function.body_start
        for index, token in enumerate(tokens)
        if token == "fn"
        for function in [_function_at(tokens, index)]
        if function is not None and function.body_start is not None
    }
    count = 0
    for index in range(1, len(tokens) - 1):
        if tokens[index : index + 2] != [type_name, "{"]:
            continue
        if tokens[index - 1] in ("struct", "impl", "for", ">"):
            continue
        if index + 1 in function_body_openings:
            continue
        count += 1
    return count


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


def _direct_named_parameters(
    function: Function, expected_type: tuple[str, ...]
) -> list[str]:
    names: list[str] = []
    for parameter in function.parameters:
        if ":" not in parameter:
            continue
        colon = parameter.index(":")
        if colon == 1 and tuple(parameter[colon + 1 :]) == expected_type:
            names.append(parameter[0])
    return names


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


def _function_body(tokens: list[str], function: Function) -> list[str]:
    if function.body_start is None or function.body_end is None:
        return []
    return tokens[function.body_start + 1 : function.body_end]


def _assignment_rhs(body: list[str], target: tuple[str, ...]) -> list[tuple[str, ...]]:
    assignments: list[tuple[str, ...]] = []
    width = len(target)
    for index in range(len(body) - width):
        if tuple(body[index : index + width]) != target or body[index + width] != "=":
            continue
        if index + width + 1 < len(body) and body[index + width + 1] == "=":
            continue
        end = index + width + 1
        while end < len(body) and body[end] != ";":
            end += 1
        assignments.append(tuple(body[index + width + 1 : end]))
    return assignments


def _struct_field(argument: tuple[str, ...], field_name: str) -> tuple[str, ...] | None:
    if tuple(argument[:3]) != ("&", "GuiInput", "{") or argument[-1:] != ("}",):
        return None
    for field in _split_top_level(list(argument[3:-1]), ","):
        if len(field) >= 3 and field[0] == field_name and field[1] == ":":
            return field[2:]
    return None


def _statement_bounds(tokens: list[str], index: int) -> tuple[int, int]:
    start = index
    while start > 0 and tokens[start - 1] not in (";", "{"):
        start -= 1
    end = index
    while end < len(tokens) and tokens[end] not in (";", "}"):
        end += 1
    return start, end


def _gui_input_aliases(tokens: list[str]) -> set[str]:
    aliases = {"gui_input"}
    changed = True
    while changed:
        changed = False
        for index, token in enumerate(tokens):
            if token != "let":
                continue
            cursor = index + 1
            if cursor < len(tokens) and tokens[cursor] == "mut":
                cursor += 1
            if cursor >= len(tokens) or not tokens[cursor].replace("_", "a").isalnum():
                continue
            name = tokens[cursor]
            while cursor < len(tokens) and tokens[cursor] not in ("=", ";"):
                cursor += 1
            if cursor >= len(tokens) or tokens[cursor] != "=":
                continue
            end = cursor + 1
            while end < len(tokens) and tokens[end] != ";":
                end += 1
            if any(alias in tokens[cursor + 1 : end] for alias in aliases) and name not in aliases:
                aliases.add(name)
                changed = True
    return aliases


def _gui_input_write_indices(tokens: list[str]) -> list[int]:
    aliases = _gui_input_aliases(tokens)
    writes: list[int] = []
    for index, token in enumerate(tokens):
        if token != "fill_uniform":
            continue
        start, end = _statement_bounds(tokens, index)
        statement = tokens[start:end]
        is_method = index > 0 and tokens[index - 1] == "."
        is_ufcs = index > 1 and tokens[index - 2 : index] == [":", ":"]
        if (is_method or is_ufcs) and any(alias in statement for alias in aliases):
            writes.append(index)
    return writes


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
            errors.append(f"owner missing one module-root struct {type_name}")
        elif any(
            "pub" in field[: field.index(":")]
            for field in _split_top_level(body, ",")
            if ":" in field
        ):
            errors.append(f"{type_name} exposes field visibility")

    for type_name in RESOLVED_TYPES:
        for path, tokens in external.items():
            if _struct_expression_count(tokens, type_name):
                errors.append(f"external construction/destructure of {type_name}: {path}")

    non_copy_assertion = (
        ":",
        ":",
        "static_assertions",
        ":",
        ":",
        "assert_not_impl_any",
        "!",
        "(",
        "ResolvedRasterLightingState",
        ":",
        ":",
        ":",
        "core",
        ":",
        ":",
        "marker",
        ":",
        ":",
        "Copy",
        ",",
        ":",
        ":",
        "core",
        ":",
        ":",
        "clone",
        ":",
        ":",
        "Clone",
        ")",
        ";",
    )
    assertion_count = sum(
        tuple(owner[index : index + len(non_copy_assertion)]) == non_copy_assertion
        for index in range(len(owner) - len(non_copy_assertion) + 1)
    )
    state_struct = _module_root_struct(owner, "ResolvedRasterLightingState")
    assertion_is_adjacent = False
    if state_struct is not None:
        state_index, _ = state_struct
        state_opening = owner.index("{", state_index + 2)
        state_closing = _closing(owner, state_opening, "{", "}")
        if state_closing is not None:
            assertion_is_adjacent = (
                tuple(
                    owner[
                        state_closing + 1 : state_closing + 1 + len(non_copy_assertion)
                    ]
                )
                == non_copy_assertion
            )
    if assertion_count != 1 or not assertion_is_adjacent:
        errors.append(
            "resolved state must be followed by one unconditional module-root "
            "rustc non-Copy/Clone assertion"
        )

    initial_state = _module_root_function(owner, "initial_raster_lighting_state")
    if initial_state is None:
        errors.append("acceptance owner must define one module-root initial raster state capability")
    else:
        initial_index, initial_function = initial_state
        if owner[max(0, initial_index - 4) : initial_index] != [
            "pub",
            "(",
            "super",
            ")",
        ]:
            errors.append("initial raster state capability must be pub(super)")
        initial_body = _function_body(owner, initial_function)
        if initial_body[-2:] == [",", "}"]:
            initial_body = initial_body[:-2] + ["}"]
        if initial_body != [
            "ResolvedRasterLightingState",
            "{",
            "raster_lighting_mode",
            ":",
            "RasterLightingMode",
            ":",
            ":",
            "Ddgi",
            "}",
        ]:
            errors.append("initial raster state capability must issue the startup DDGI state")
    initial_state_sites = [
        path
        for path, tokens in external.items()
        for _ in range(tokens.count("initial_raster_lighting_state"))
    ]
    if initial_state_sites != [CALLER]:
        errors.append(
            f"initial raster state capability references must be exactly [{CALLER}], "
            f"got {initial_state_sites}"
        )

    state_factory = _canonical_function(
        owner, "ResolvedLightingFrameInputs", "raster_lighting_state"
    )
    if state_factory is None:
        errors.append(
            "ResolvedLightingFrameInputs must own exactly one raster_lighting_state factory"
        )
    else:
        if ("&", "self") not in state_factory.parameters:
            errors.append("raster_lighting_state factory must borrow its capsule")
        factory_body = _function_body(owner, state_factory)
        if factory_body[-2:] == [",", "}"]:
            factory_body = factory_body[:-2] + ["}"]
        if factory_body != [
            "ResolvedRasterLightingState",
            "{",
            "raster_lighting_mode",
            ":",
            "self",
            ".",
            "raster_lighting_mode",
            "}",
        ]:
            errors.append("resolved raster state must be constructed only from its capsule field")
    state_observer = _canonical_function(
        owner, "ResolvedRasterLightingState", "is_ddgi"
    )
    if state_observer is None or ("&", "self") not in state_observer.parameters:
        errors.append("ResolvedRasterLightingState::is_ddgi must borrow the opaque state")
    if _struct_expression_count(owner, "ResolvedRasterLightingState") != 2:
        errors.append(
            "owner must contain only initial and per-frame resolved raster state construction"
        )

    caller_tokens = tokenized.get(CALLER, [])
    app_new = _canonical_function(caller_tokens, "App", "new")
    if app_new is None:
        errors.append("App inherent impl must own exactly one canonical new entry")
    else:
        app_new_body = _function_body(caller_tokens, app_new)
        tracer_calls = _call_arguments(app_new_body, ("Tracer", ":", ":", "new", "("))
        initial_argument = (
            "lighting_mode_acceptance",
            ":",
            ":",
            "initial_raster_lighting_state",
            "(",
            ")",
        )
        if len(tracer_calls) != 1 or initial_argument not in tracer_calls[0]:
            errors.append("App::new must move the owner-issued initial state into Tracer::new")

    tracer_tokens = tokenized.get(TRACER_OWNER, [])
    if "RasterLightingMode" in tracer_tokens:
        errors.append("Tracer must not store or accept raw RasterLightingMode")
    if "initial_raster_lighting_state" in tracer_tokens:
        errors.append("Tracer must not call the owner-only initial state capability")
    tracer_new = _canonical_function(tracer_tokens, "Tracer", "new")
    if tracer_new is None:
        errors.append("Tracer inherent impl must own exactly one new entry")
    elif len(
        _direct_named_parameters(tracer_new, ("ResolvedRasterLightingState",))
    ) != 1:
        errors.append("Tracer::new requires one directly moved ResolvedRasterLightingState")
    for function in _inherent_impl_functions(tracer_tokens, "Tracer"):
        if function.name != "new" and any(
            "ResolvedRasterLightingState" in parameter
            for parameter in function.parameters
        ):
            errors.append(
                f"Tracer::{function.name} must not accept another resolved raster state"
            )
    update_buffers = _canonical_function(tracer_tokens, "Tracer", "update_buffers")
    if update_buffers is None:
        errors.append("Tracer inherent impl must own exactly one update_buffers entry")
    else:
        if update_buffers.generic_tokens:
            errors.append("Tracer::update_buffers must use the production non-generic entry")
        if ("&", "mut", "self") not in update_buffers.parameters:
            errors.append("Tracer::update_buffers requires &mut self receiver")
        lighting_parameters = _direct_named_parameters(
            update_buffers, ("&", "ResolvedLightingFrameInputs")
        )
        if len(lighting_parameters) != 1:
            errors.append(
                "Tracer::update_buffers requires exactly one direct "
                "&ResolvedLightingFrameInputs parameter"
            )
        else:
            lighting_parameter = lighting_parameters[0]
            body = _function_body(tracer_tokens, update_buffers)
            primitive_names = {
                "raster_lighting_mode",
                "path_tracing_reference",
                "path_tracing_max_bounces",
                "path_tracing_ambient_light",
            }
            primitive_types = {
                "RasterLightingMode",
                "TerrainLightingMode",
                "EffectiveLightingControls",
            }
            if any(
                any(type_name in parameter for type_name in primitive_types)
                or (parameter and parameter[0] in primitive_names)
                for parameter in update_buffers.parameters
            ):
                errors.append("Tracer::update_buffers exposes a primitive lighting bypass")
            tracer_struct = _struct_body(tracer_tokens, "Tracer")
            fields = [] if tracer_struct is None else _split_top_level(tracer_struct, ",")
            expected_state_field = (
                "raster_lighting_state",
                ":",
                "ResolvedRasterLightingState",
            )
            if fields.count(expected_state_field) != 1:
                errors.append("Tracer must store exactly one opaque resolved raster state")
            mode_assignments = _assignment_rhs(body, ("self", ".", "raster_lighting_state"))
            expected_mode = (
                lighting_parameter,
                ".",
                "raster_lighting_state",
                "(",
                ")",
            )
            if mode_assignments != [expected_mode]:
                errors.append(
                    "Tracer opaque raster state must be assigned once from its capsule factory"
                )
            all_state_assignments = _assignment_rhs(
                tracer_tokens, ("self", ".", "raster_lighting_state")
            )
            if all_state_assignments != [expected_mode]:
                errors.append("Tracer must contain no second raster state write")

    updater_tokens = tokenized.get(BUFFER_UPDATER_OWNER, [])
    update_gui_input = _canonical_function(updater_tokens, "BufferUpdater", "update_gui_input")
    if update_gui_input is None:
        errors.append("BufferUpdater inherent impl must own exactly one update_gui_input entry")
    else:
        if update_gui_input.generic_tokens:
            errors.append("BufferUpdater::update_gui_input must use the production non-generic entry")
        if any("self" in parameter for parameter in update_gui_input.parameters):
            errors.append("BufferUpdater::update_gui_input must remain an associated function")
        lighting_parameters = _direct_named_parameters(
            update_gui_input, ("&", "ResolvedLightingFrameInputs")
        )
        resource_parameters = _direct_named_parameters(
            update_gui_input, ("&", "TracerResources")
        )
        if len(lighting_parameters) != 1:
            errors.append(
                "BufferUpdater::update_gui_input requires exactly one direct "
                "&ResolvedLightingFrameInputs parameter"
            )
        if len(resource_parameters) != 1:
            errors.append(
                "BufferUpdater::update_gui_input requires exactly one direct &TracerResources "
                "parameter"
            )
        primitive_names = {
            "raster_lighting_mode",
            "path_tracing_reference",
            "path_tracing_max_bounces",
            "path_tracing_ambient_light",
        }
        if any(
            "RasterLightingMode" in parameter
            or (parameter and parameter[0] in primitive_names)
            for parameter in update_gui_input.parameters
        ):
            errors.append("BufferUpdater::update_gui_input exposes a primitive lighting bypass")
        if len(lighting_parameters) == 1 and len(resource_parameters) == 1:
            lighting_parameter = lighting_parameters[0]
            resources_parameter = resource_parameters[0]
            body = _function_body(updater_tokens, update_gui_input)
            sink_calls = _call_arguments(
                body,
                (
                    resources_parameter,
                    ".",
                    "uniforms",
                    ".",
                    "gui_input",
                    ".",
                    "fill_uniform",
                    "(",
                ),
            )
            fill_uniform_reference_count = sum(token == "fill_uniform" for token in body)
            if (
                fill_uniform_reference_count != 1
                or len(sink_calls) != 1
                or len(sink_calls[0]) != 1
            ):
                errors.append("BufferUpdater::update_gui_input requires one direct inline GUI sink")
            else:
                argument = sink_calls[0][0]
                expected_fields = {
                    "raster_flora_ddgi_lighting": (
                        lighting_parameter,
                        ".",
                        "raster_lighting_mode",
                        "(",
                        ")",
                        ".",
                        "is_ddgi",
                        "(",
                        ")",
                        "as",
                        "u32",
                    ),
                    "path_tracing_reference": (
                        lighting_parameter,
                        ".",
                        "path_tracing_reference",
                        "(",
                        ")",
                        "as",
                        "u32",
                    ),
                    "path_tracing_max_bounces": (
                        lighting_parameter,
                        ".",
                        "path_tracing_max_bounces",
                        "(",
                        ")",
                    ),
                    "path_tracing_ambient_light": (
                        lighting_parameter,
                        ".",
                        "path_tracing_ambient_light",
                        "(",
                        ")",
                        ".",
                        "to_array",
                        "(",
                        ")",
                    ),
                }
                for field_name, expected in expected_fields.items():
                    if _struct_field(argument, field_name) != expected:
                        errors.append(
                            f"GUI uniform field {field_name} must derive inline from "
                            f"{lighting_parameter}"
                        )

    write_sites: list[tuple[str, int]] = []
    for path, tokens in tokenized.items():
        write_sites.extend((path, index) for index in _gui_input_write_indices(tokens))
    if len(write_sites) != 1:
        errors.append(f"current production source must contain one gui_input write, got {write_sites}")
    elif update_gui_input is None or write_sites[0][0] != BUFFER_UPDATER_OWNER:
        errors.append("current gui_input write must belong to BufferUpdater::update_gui_input")
    elif not (
        update_gui_input.body_start is not None
        and update_gui_input.body_end is not None
        and update_gui_input.body_start < write_sites[0][1] < update_gui_input.body_end
    ):
        errors.append("current gui_input write must be inside BufferUpdater::update_gui_input")

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
