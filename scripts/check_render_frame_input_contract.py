#!/usr/bin/env python3
"""Guard the typed frame boundary between App and Tracer.

This narrow source contract prevents the render frame seam from regressing to a flat list of GUI
values. Rust compilation remains authoritative for the values carried by each typed snapshot.
"""

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def function_parameters(source: str, function_name: str) -> list[str]:
    marker = f"fn {function_name}("
    start = source.index(marker) + len(marker)
    depth = 0
    parameter_start = start
    parameters: list[str] = []
    for index in range(start, len(source)):
        character = source[index]
        if character in "(<[{":
            depth += 1
        elif character in ")>]}":
            if character == ")" and depth == 0:
                final = source[parameter_start:index].strip()
                if final:
                    parameters.append(final)
                return parameters
            depth -= 1
        elif character == "," and depth == 0:
            parameters.append(source[parameter_start:index].strip())
            parameter_start = index + 1
    raise ValueError(f"unterminated signature for {function_name}")


def validate(tracer: str, buffer_updater: str, app: str) -> list[str]:
    errors: list[str] = []
    frame_parameters = function_parameters(tracer, "update_buffers")
    gui_parameters = function_parameters(buffer_updater, "update_gui_input")

    if len(frame_parameters) > 10:
        errors.append(f"Tracer::update_buffers exposes {len(frame_parameters)} parameters (max 10)")
    if len(gui_parameters) > 7:
        errors.append(f"BufferUpdater::update_gui_input exposes {len(gui_parameters)} parameters (max 7)")

    required_snapshots = (
        "TerrainFrameInput",
        "MaterialFrameInput",
        "VegetationFrameInput",
        "WindFrameInput",
        "EnvironmentFrameInput",
    )
    for snapshot in required_snapshots:
        if f"pub struct {snapshot}" not in tracer:
            errors.append(f"missing typed snapshot {snapshot}")
        if snapshot not in app:
            errors.append(f"App does not freeze {snapshot}")

    update_start = tracer.index("pub fn update_buffers")
    if "too_many_arguments" in tracer[max(0, update_start - 100) : update_start]:
        errors.append("Tracer::update_buffers still suppresses too_many_arguments")
    return errors


def main() -> int:
    errors = validate(
        (ROOT / "src/tracer/mod.rs").read_text(encoding="utf-8"),
        (ROOT / "src/tracer/buffer_updater.rs").read_text(encoding="utf-8"),
        (ROOT / "src/app/core/mod.rs").read_text(encoding="utf-8")
        + (ROOT / "src/app/core/render_frame_input.rs").read_text(encoding="utf-8"),
    )
    if errors:
        for error in errors:
            print(f"render frame input contract: {error}")
        return 1
    print("render frame input contract: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
