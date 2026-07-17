#!/usr/bin/env python3
"""Verify that the Slang roadmap matches the shader tree and build overrides."""

from __future__ import annotations

import re
import sys
from collections import Counter
from pathlib import Path
from typing import NoReturn

ROOT = Path(__file__).resolve().parents[1]
ROADMAP = ROOT / "docs/slang-migration-roadmap.md"
BUILD_RS = ROOT / "crates/re-flora-vkn/build.rs"
SHADER_SUFFIXES = {".comp", ".vert", ".frag"}
ENTRY_RE = re.compile(r"^- \[([ x])\] `(shader/[^`]+\.(?:comp|vert|frag))`([^\n]*)$", re.MULTILINE)
OVERRIDE_RE = re.compile(r"ShaderOverride\s*\{(.*?)\n\s*\},", re.DOTALL)


def fail(message: str) -> NoReturn:
    print(message, file=sys.stderr)
    raise SystemExit(1)


def summary_count(text: str, label: str) -> int:
    match = re.search(rf"^\| {re.escape(label)} \| (\d+) \|", text, re.MULTILINE)
    if match is None:
        fail(f"missing roadmap summary row: {label}")
    return int(match.group(1))


def build_overrides(text: str) -> tuple[set[str], set[str]]:
    native: set[str] = set()
    backend: set[str] = set()
    for block in OVERRIDE_RE.findall(text):
        path_match = re.search(r'logical_path:\s*"([^"]+)"', block)
        frontend_match = re.search(r"frontend:\s*ShaderFrontend::(\w+)", block)
        if path_match is None or frontend_match is None:
            fail("could not parse a ShaderOverride block in crates/re-flora-vkn/build.rs")
        logical_path = path_match.group(1)
        frontend = frontend_match.group(1)
        if frontend == "NativeSlang2025":
            target = native
        elif frontend == "GlslViaSlang":
            target = backend
        else:
            fail(f"unknown shader frontend {frontend} for {logical_path}")
        if logical_path in target:
            fail(f"duplicate {frontend} shader override: {logical_path}")
        target.add(logical_path)
    return native, backend


def main() -> int:
    roadmap_text = ROADMAP.read_text(encoding="utf-8")
    try:
        inventory_text = roadmap_text.split("## Entry-point inventory", 1)[1].split(
            "## Known risks", 1
        )[0]
    except IndexError:
        fail("roadmap is missing the entry-point inventory boundaries")

    shader_paths = {
        path.relative_to(ROOT).as_posix()
        for path in (ROOT / "shader").rglob("*")
        if path.is_file() and path.suffix in SHADER_SUFFIXES
    }
    entries = ENTRY_RE.findall(inventory_text)
    entry_counts = Counter(path for _checked, path, _note in entries)
    duplicate_paths = sorted(path for path, count in entry_counts.items() if count != 1)
    if duplicate_paths:
        fail(f"roadmap inventory contains duplicate paths: {duplicate_paths}")

    roadmap_paths = set(entry_counts)
    missing = sorted(shader_paths - roadmap_paths)
    extra = sorted(roadmap_paths - shader_paths)
    if missing or extra:
        fail(f"roadmap inventory mismatch: missing={missing}, extra={extra}")

    checked_paths = {path for checked, path, _note in entries if checked == "x"}
    backend_paths = {
        path for checked, path, note in entries if checked == " " and "backend validated" in note
    }
    native_overrides, backend_overrides = build_overrides(BUILD_RS.read_text(encoding="utf-8"))
    if checked_paths != native_overrides:
        fail(
            "native roadmap/build override mismatch: "
            f"roadmap_only={sorted(checked_paths - native_overrides)}, "
            f"build_only={sorted(native_overrides - checked_paths)}"
        )
    backend_only_overrides = backend_overrides - native_overrides
    if backend_paths != backend_only_overrides:
        fail(
            "backend roadmap/build override mismatch: "
            f"roadmap_only={sorted(backend_paths - backend_only_overrides)}, "
            f"build_only={sorted(backend_only_overrides - backend_paths)}"
        )

    native_summary = summary_count(roadmap_text, "Native Slang complete")
    backend_summary = summary_count(roadmap_text, "Slang backend only")
    glsl_summary = summary_count(roadmap_text, "GLSL only")
    actual_counts = (len(checked_paths), len(backend_paths), len(shader_paths - checked_paths - backend_paths))
    summary_counts = (native_summary, backend_summary, glsl_summary)
    if summary_counts != actual_counts:
        fail(f"roadmap summary counts {summary_counts} do not match inventory {actual_counts}")

    print(
        f"Slang roadmap matches {len(shader_paths)} shader entry points: "
        f"{len(checked_paths)} native, {len(backend_paths)} backend-only, "
        f"{actual_counts[2]} GLSL-only"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
