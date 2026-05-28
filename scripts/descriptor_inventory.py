#!/usr/bin/env python3
"""Inventory shader image descriptors from GLSL source.

This is a lightweight tracker for the sampled-image refactor. It counts GLSL
image declarations as storage images and sampler declarations as sampled-image
style descriptors. It is intentionally source based so it can run without a GPU
or shader compiler; Vulkan reflection remains the final source of truth.
"""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path


DECL_RE = re.compile(
    r"layout\s*\((?P<layout>[^)]*)\)\s*"
    r"(?P<qualifiers>(?:(?:readonly|writeonly|coherent|volatile|restrict)\s+)*)"
    r"uniform\s+"
    r"(?P<ty>[iu]?image\w+|[iu]?sampler\w+)\s+"
    r"(?P<name>\w+)\s*;"
)
INCLUDE_RE = re.compile(r'^\s*#include\s+"(?P<path>[^\"]+)"', re.MULTILINE)
STORE_RE = re.compile(r"\bimageStore\s*\(\s*(?P<name>\w+)\b")
LOAD_RE = re.compile(r"\bimageLoad\s*\(\s*(?P<name>\w+)\b")
FETCH_RE = re.compile(r"\btexelFetch\s*\(\s*(?P<name>\w+)\b")
TEXTURE_RE = re.compile(r"\btexture(?:Lod|Grad|Proj)?\s*\(\s*(?P<name>\w+)\b")


@dataclass(frozen=True)
class DescriptorDecl:
    shader: Path
    set_no: str
    binding_no: str
    descriptor_class: str
    glsl_type: str
    name: str
    qualifiers: str
    format: str
    access: str


def read_with_includes(path: Path, seen: set[Path] | None = None) -> str:
    path = path.resolve()
    seen = seen or set()
    if path in seen:
        return ""
    seen.add(path)
    text = path.read_text()

    def replace_include(match: re.Match[str]) -> str:
        include_path = (path.parent / match.group("path")).resolve()
        if not include_path.exists():
            return match.group(0)
        return read_with_includes(include_path, seen)

    return INCLUDE_RE.sub(replace_include, text)


def layout_value(layout: str, key: str) -> str:
    match = re.search(rf"\b{re.escape(key)}\s*=\s*([^,\s]+)", layout)
    return match.group(1) if match else "-"


def layout_format(layout: str) -> str:
    items = [part.strip() for part in layout.split(",")]
    for item in items:
        if item and "=" not in item and not item.startswith("local_size"):
            return item
    return "-"


def classify_access(name: str, qualifiers: str, text: str, descriptor_class: str) -> str:
    if descriptor_class == "sampled":
        fetches = bool(re.search(rf"\btexelFetch\s*\(\s*{re.escape(name)}\b", text))
        textures = bool(
            re.search(rf"\btexture(?:Lod|Grad|Proj)?\s*\(\s*{re.escape(name)}\b", text)
        )
        if fetches or textures:
            return "sampled-read"
        return "declared"
    stores = bool(re.search(rf"\bimageStore\s*\(\s*{re.escape(name)}\b", text))
    loads = bool(re.search(rf"\bimageLoad\s*\(\s*{re.escape(name)}\b", text))
    if stores and loads:
        return "read-write"
    if stores:
        return "write"
    if loads:
        return "read"
    if "writeonly" in qualifiers:
        return "writeonly-declared"
    if "readonly" in qualifiers:
        return "readonly-declared"
    return "declared"


def inventory_shader(path: Path) -> list[DescriptorDecl]:
    text = read_with_includes(path)
    decls: list[DescriptorDecl] = []
    for match in DECL_RE.finditer(text):
        ty = match.group("ty")
        descriptor_class = "storage" if "image" in ty else "sampled"
        qualifiers = " ".join(match.group("qualifiers").split())
        name = match.group("name")
        decls.append(
            DescriptorDecl(
                shader=path,
                set_no=layout_value(match.group("layout"), "set"),
                binding_no=layout_value(match.group("layout"), "binding"),
                descriptor_class=descriptor_class,
                glsl_type=ty,
                name=name,
                qualifiers=qualifiers or "-",
                format=layout_format(match.group("layout")),
                access=classify_access(name, qualifiers, text, descriptor_class),
            )
        )
    return sorted(decls, key=lambda d: (int(d.set_no) if d.set_no.isdigit() else 999, int(d.binding_no) if d.binding_no.isdigit() else 999, d.name))


def print_markdown(shader_decls: dict[Path, list[DescriptorDecl]]) -> None:
    print("| shader | storage | sampled | storage read-only/dead | storage write-capable |")
    print("| --- | ---: | ---: | ---: | ---: |")
    for shader, decls in shader_decls.items():
        storage = [d for d in decls if d.descriptor_class == "storage"]
        sampled = [d for d in decls if d.descriptor_class == "sampled"]
        readonly = [d for d in storage if d.access in {"read", "readonly-declared", "declared"}]
        write_capable = [d for d in storage if d.access in {"write", "read-write", "writeonly-declared"}]
        print(
            f"| `{shader}` | {len(storage)} | {len(sampled)} | "
            f"{len(readonly)} | {len(write_capable)} |"
        )

    print()
    for shader, decls in shader_decls.items():
        print(f"### {shader}")
        print()
        print("| set | binding | class | type | format | access | name | qualifiers |")
        print("| ---: | ---: | --- | --- | --- | --- | --- | --- |")
        for d in decls:
            print(
                f"| {d.set_no} | {d.binding_no} | {d.descriptor_class} | `{d.glsl_type}` | "
                f"`{d.format}` | {d.access} | `{d.name}` | {d.qualifiers} |"
            )
        print()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("shaders", nargs="+", type=Path)
    args = parser.parse_args()

    shader_decls = {path: inventory_shader(path) for path in args.shaders}
    print_markdown(shader_decls)


if __name__ == "__main__":
    main()
