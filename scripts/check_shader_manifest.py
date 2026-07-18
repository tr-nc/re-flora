#!/usr/bin/env python3
"""Verify the native Slang shader tree and build manifest."""

from __future__ import annotations

import re
import sys
from collections import Counter
from pathlib import Path
from typing import NoReturn

ROOT = Path(__file__).resolve().parents[1]
SHADER_ROOT = ROOT / "shader"
MANIFEST = ROOT / "crates/re-flora-shader-build/src/lib.rs"
CONFIG_RE = re.compile(
    r"ShaderConfig\s*\{\s*"
    r'logical_path:\s*"([^"]+)",\s*'
    r'source_path:\s*"([^"]+)",\s*'
    r'module_path:\s*"([^"]+)",\s*'
    r"stage:\s*ShaderStage::(Compute|Vertex|Fragment),\s*"
    r"\}",
    re.DOTALL,
)
STAGE_EXTENSION = {"Compute": ".comp", "Vertex": ".vert", "Fragment": ".frag"}


def fail(message: str) -> NoReturn:
    print(message, file=sys.stderr)
    raise SystemExit(1)


def duplicates(values: list[str]) -> list[str]:
    return sorted(value for value, count in Counter(values).items() if count != 1)


def main() -> int:
    shader_files = sorted(path for path in SHADER_ROOT.rglob("*") if path.is_file())
    non_slang = [path.relative_to(ROOT).as_posix() for path in shader_files if path.suffix != ".slang"]
    if non_slang:
        fail(f"non-Slang shader sources remain: {non_slang}")

    configs = CONFIG_RE.findall(MANIFEST.read_text(encoding="utf-8"))
    if not configs:
        fail("native shader manifest is empty or could not be parsed")

    logical_paths = [logical_path for logical_path, _source, _include, _stage in configs]
    source_paths = [source_path for _logical, source_path, _include, _stage in configs]
    duplicate_logical_paths = duplicates(logical_paths)
    duplicate_source_paths = duplicates(source_paths)
    if duplicate_logical_paths or duplicate_source_paths:
        fail(
            "duplicate native shader manifest entries: "
            f"logical={duplicate_logical_paths}, source={duplicate_source_paths}"
        )

    module_names: list[str] = []
    imported_modules: set[str] = set()
    for shader_file in shader_files:
        source = shader_file.read_text(encoding="utf-8")
        if re.search(r"^\s*#\s*include\b", source, re.MULTILINE):
            fail(f"textual include remains in {shader_file.relative_to(ROOT).as_posix()}")
        relative_path = shader_file.relative_to(ROOT).as_posix()
        module = re.search(
            r"^\s*module\s+([A-Za-z_][A-Za-z0-9_]*)\s*;", source, re.MULTILINE
        )
        if relative_path not in source_paths:
            if module is None:
                fail(f"shared Slang source is not a module: {relative_path}")
            module_names.append(module.group(1))
        imported_modules.update(
            re.findall(
                r"^\s*import\s+([A-Za-z_][A-Za-z0-9_]*)\s*;", source, re.MULTILINE
            )
        )

    duplicate_modules = duplicates(module_names)
    if duplicate_modules:
        fail(f"duplicate shared Slang module names: {duplicate_modules}")
    module_name_set = set(module_names)
    missing_modules = sorted(imported_modules - module_name_set)
    unused_modules = sorted(module_name_set - imported_modules)
    if missing_modules or unused_modules:
        fail(
            "Slang module graph mismatch: "
            f"missing={missing_modules}, unused={unused_modules}"
        )

    for logical_path, source_path, module_path, stage in configs:
        if Path(logical_path).suffix != STAGE_EXTENSION[stage]:
            fail(f"stage mismatch for {logical_path}: {stage}")
        source = ROOT / source_path
        if source.suffix != ".slang" or not source.is_file():
            fail(f"missing native Slang entry point: {source_path}")
        module_directory = ROOT / module_path
        if not module_directory.is_dir():
            fail(f"missing native Slang module directory: {module_path}")

    print(
        f"native shader manifest matches {len(configs)} entry points "
        f"and {len(shader_files) - len(configs)} shared Slang modules"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
