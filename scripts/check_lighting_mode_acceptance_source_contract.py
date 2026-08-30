#!/usr/bin/env python3
from __future__ import annotations

import sys
from pathlib import Path


OWNER = "src/app/core/lighting_mode_acceptance.rs"
CALLER = "src/app/core/mod.rs"
ENVIRONMENT_OWNER = "src/environment_lighting.rs"
RESOLVED_TYPES = ("ResolvedLightingFrameInputs", "ResolvedFrameTiming")
PLAN_METHODS = ("frame_plan", "resolve_timing", "resolve_lighting")


def read_sources(src_root: Path) -> dict[str, str]:
    repo_root = src_root.parent
    return {
        path.relative_to(repo_root).as_posix(): path.read_text()
        for path in sorted(src_root.rglob("*.rs"))
    }


def tokenize(source: str) -> list[str]:
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

        raw_start = index
        if source.startswith("br", raw_start):
            raw_start += 2
        elif source.startswith("r", raw_start):
            raw_start += 1
        else:
            raw_start = -1
        if raw_start >= 0:
            quote = raw_start
            while quote < length and source[quote] == "#":
                quote += 1
            if quote < length and source[quote] == '"':
                hashes = quote - raw_start
                terminator = '"' + ("#" * hashes)
                end = source.find(terminator, quote + 1)
                index = length if end < 0 else end + len(terminator)
                continue

        string_quote = index
        if char in ("b", "c") and index + 1 < length and source[index + 1] == '"':
            string_quote = index + 1
        if source[string_quote] == '"':
            index = string_quote + 1
            while index < length:
                if source[index] == "\\":
                    index += 2
                elif source[index] == '"':
                    index += 1
                    break
                else:
                    index += 1
            continue

        if char == "'":
            if index + 2 < length and source[index + 2] == "'":
                index += 3
                continue
            if index + 1 < length and source[index + 1] == "\\":
                end = index + 2
                while end < length and source[end] not in ("'", "\n"):
                    end += 1
                if end < length and source[end] == "'":
                    index = end + 1
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


def _contains(tokens: list[str], sequence: tuple[str, ...]) -> bool:
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


def _function_parameters(tokens: list[str], name: str) -> list[list[str]]:
    parameters: list[list[str]] = []
    for index in range(len(tokens) - 2):
        if tokens[index] != "fn" or tokens[index + 1] != name or tokens[index + 2] != "(":
            continue
        closing = _closing(tokens, index + 2, "(", ")")
        if closing is not None:
            parameters.append(tokens[index + 3 : closing])
    return parameters


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

    update_declarations = [
        (path, parameters)
        for path, tokens in external.items()
        for parameters in _function_parameters(tokens, "update_buffers")
    ]
    if len(update_declarations) != 1 or update_declarations[0][0] != "src/tracer/mod.rs":
        errors.append(
            "update_buffers declaration must be unique in src/tracer/mod.rs, got "
            f"{[path for path, _ in update_declarations]}"
        )
    elif not _contains(update_declarations[0][1], ("&", "ResolvedLightingFrameInputs")):
        errors.append("update_buffers lacks typed ResolvedLightingFrameInputs capability")

    gui_declarations = [
        (path, parameters)
        for path, tokens in external.items()
        for parameters in _function_parameters(tokens, "update_gui_input")
    ]
    if len(gui_declarations) != 1 or gui_declarations[0][0] != "src/tracer/buffer_updater.rs":
        errors.append(
            "update_gui_input declaration must be unique in src/tracer/buffer_updater.rs, got "
            f"{[path for path, _ in gui_declarations]}"
        )
    elif "RasterLightingMode" not in gui_declarations[0][1]:
        errors.append("update_gui_input lacks typed RasterLightingMode capability")

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
