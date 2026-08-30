#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path


OWNER = "src/app/core/lighting_mode_acceptance.rs"
CALLER = "src/app/core/mod.rs"
ENVIRONMENT_OWNER = "src/environment_lighting.rs"


def read_sources(src_root: Path) -> dict[str, str]:
    repo_root = src_root.parent
    return {
        path.relative_to(repo_root).as_posix(): path.read_text()
        for path in sorted(src_root.rglob("*.rs"))
    }


def _function_signatures(source: str, name: str) -> list[str]:
    return re.findall(
        rf"pub\s+fn\s+{re.escape(name)}\s*\((.*?)\)\s*->\s*Result<\(\)>",
        source,
        flags=re.DOTALL,
    )


def _struct_body(source: str, name: str) -> str | None:
    match = re.search(rf"\bstruct\s+{re.escape(name)}\s*\{{(.*?)\n\}}", source, re.DOTALL)
    return match.group(1) if match else None


def audit(sources: dict[str, str]) -> list[str]:
    errors: list[str] = []
    owner = sources.get(OWNER)
    if owner is None:
        return [f"missing owner: {OWNER}"]
    external = {path: source for path, source in sources.items() if path != OWNER}

    for type_name in ("ResolvedLightingFrameInputs", "ResolvedFrameTiming"):
        body = _struct_body(owner, type_name)
        if body is None:
            errors.append(f"owner missing struct {type_name}")
            continue
        for line in body.splitlines():
            if line.lstrip().startswith("pub"):
                errors.append(f"{type_name} exposes field visibility: {line.strip()}")

    for type_name in ("ResolvedLightingFrameInputs", "ResolvedFrameTiming"):
        literal = re.compile(rf"\b{type_name}\s*\{{")
        for path, source in external.items():
            if literal.search(source):
                errors.append(f"external construction/destructure of {type_name}: {path}")

    for token in (".frame_plan(", ".resolve_timing(", ".resolve_lighting("):
        sites = [
            path
            for path, source in external.items()
            for _ in range(source.count(token))
        ]
        if sites != [CALLER]:
            errors.append(f"{token} call sites must be exactly [{CALLER}], got {sites}")

    primitive_update_parameters = (
        "frame_serial_idx: u32",
        "dither_strength_lsb: f32",
        "raster_flora_ddgi_lighting: bool",
        "path_tracing_reference: bool",
        "path_tracing_max_bounces: u32",
        "path_tracing_ambient_light: Vec3",
    )
    typed_update_signatures: list[tuple[str, str]] = []
    for path, source in external.items():
        for signature in _function_signatures(source, "update_buffers"):
            if "ResolvedLightingFrameInputs" in signature:
                typed_update_signatures.append((path, signature))
            for primitive in primitive_update_parameters:
                if primitive in signature:
                    errors.append(f"primitive update_buffers bypass {primitive}: {path}")
    if len(typed_update_signatures) != 1:
        errors.append(
            "expected exactly one typed update_buffers consumer, got "
            f"{[path for path, _ in typed_update_signatures]}"
        )

    buffer_signatures = [
        (path, signature)
        for path, source in external.items()
        for signature in _function_signatures(source, "update_gui_input")
    ]
    typed_buffer_signatures = [
        (path, signature)
        for path, signature in buffer_signatures
        if "raster_lighting_mode: RasterLightingMode" in signature
    ]
    if len(typed_buffer_signatures) != 1:
        errors.append(
            "expected exactly one typed update_gui_input sink, got "
            f"{[path for path, _ in typed_buffer_signatures]}"
        )
    for path, signature in buffer_signatures:
        if "raster_flora_ddgi_lighting: bool" in signature:
            errors.append(f"raster mode lowered before GPU sink: {path}")

    if "ResolvedLightingFrameInputs" in sources.get(ENVIRONMENT_OWNER, ""):
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
