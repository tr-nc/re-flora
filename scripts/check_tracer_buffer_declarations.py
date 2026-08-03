#!/usr/bin/env python3
"""Static guardrail for the tracer Buffer recording seam.

This is intentionally a narrow source contract, not a substitute for the release hidden run. It
keeps future tracer edits from silently bypassing the shared command-recording BufferUse seam while
the rendergraph-lite pass order remains explicit.
"""

from pathlib import Path
from typing import Optional


ROOT = Path(__file__).resolve().parents[1]
SOURCE = (ROOT / "src/tracer/mod.rs").read_text(encoding="utf-8")


def section(name: str, next_name: Optional[str]) -> str:
    start = SOURCE.index(name)
    end = SOURCE.index(next_name, start + 1) if next_name else len(SOURCE)
    return SOURCE[start:end]


updated = section("pub fn record_updated_buffer_uses", "/// Declares builder-owned")
graphics = section("fn record_graphics_buffer_uses", "#[allow(clippy::too_many_arguments)]")
shadow = section("pub fn record_shadow_prepass", "pub fn record_trace_after_shadow_prepass")
capture_readback = section(
    "pub fn record_environment_irradiance_capture_readback",
    "pub fn environment_probe_terrain_revision_ready",
)
terrain_query = section("fn query_terrain_rays_chunk_with_validity", None)

required = {
    "updated HostWrite/ShaderRead": (updated, "BufferUse::HostWrite", "BufferUse::ShaderRead"),
    "graphics VertexRead": (graphics, "BufferUse::VertexRead"),
    "graphics IndexRead": (graphics, "BufferUse::IndexRead"),
    "graphics instance HostWrite": (graphics, "BufferUse::HostWrite"),
    "DDGI ComputeWrite": (shadow, "BufferUse::ComputeWrite"),
    "DDGI ComputeRead": (shadow, "BufferUse::ComputeRead"),
    "capture readback HostRead": (capture_readback, "BufferUse::HostRead"),
    "terrain query HostRead": (terrain_query, "BufferUse::HostRead"),
}

missing = []
for label, values in required.items():
    if not all(value in values[0] for value in values[1:]):
        missing.append(label)

if "record_contree_buffer_uses(cmdbuf);" not in shadow:
    missing.append("shadow pass Contree declaration seam")
if "PipelineBarrier::compute_shader_access" in SOURCE:
    missing.append("superseded tracer compute-only fallback barrier")
if ".record_indirect(" in SOURCE:
    missing.append("unscoped tracer indirect dispatch")

if missing:
    raise SystemExit("tracer Buffer declaration contract failed: " + ", ".join(missing))

print("tracer Buffer declaration contract passed")
