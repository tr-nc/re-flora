#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path

import analyze_environment_irradiance_capture as analyzer


CHECKPOINTS = ("baseline", "r2-next-frame", "r4-next-frame", "final")


def checkpoint_path(base: Path, checkpoint: str) -> Path:
    if checkpoint == "final":
        return base
    return base.with_name(f"{base.stem}.{checkpoint}{base.suffix}")


def load_identity(capture_path: Path) -> dict[str, object]:
    identity_path = Path(f"{capture_path}.identity.json")
    with identity_path.open(encoding="utf-8") as identity_file:
        return json.load(identity_file)


def require(condition: bool, message: str, failures: list[str]) -> None:
    if not condition:
        failures.append(message)


def require_current_capture(
    capture: analyzer.Capture, checkpoint: str, failures: list[str]
) -> None:
    require(
        analyzer.is_current_capture(capture),
        f"{checkpoint}: capture is not current RFIRR",
        failures,
    )
    require(
        capture.filter_evidence is not None
        and capture.grid_dimensions is not None
        and capture.configured_history_retention_q16 is not None,
        f"{checkpoint}: current DDGI filter proof is incomplete",
        failures,
    )


def require_required_planes_finite(
    capture: analyzer.Capture, checkpoint: str, failures: list[str]
) -> None:
    require(
        analyzer.required_capture_planes_finite(capture),
        f"{checkpoint}: required capture planes contain non-finite values",
        failures,
    )


def field_matches_capture(
    field: dict[str, object], capture: analyzer.Capture
) -> bool:
    lifecycle_state = analyzer.LIFECYCLE_STATE_LABELS.get(capture.lifecycle_state)
    return (
        field["field_serial"] == capture.field_serial
        and field["geometry_revision"] == capture.geometry_revision
        and field["radiance_revision"] == capture.radiance_revision
        and field["spacing_voxels"] == capture.spacing_voxels
        and str(field["lifecycle_state"]).lower() == lifecycle_state
        and field["update_epoch"] == capture.update_epoch
        and field["source_field_serial"] == capture.source_field_serial
        and field["source_radiance_revision"] == capture.source_radiance_revision
    )


def validate(
    base: Path,
    spacing_voxels: int,
    sunlit_roi: tuple[float, float, float, float, float, float],
    min_direct_delta: float,
) -> dict[str, object]:
    failures: list[str] = []
    paths = {checkpoint: checkpoint_path(base, checkpoint) for checkpoint in CHECKPOINTS}
    captures = {checkpoint: analyzer.load_capture(path) for checkpoint, path in paths.items()}
    identities = {checkpoint: load_identity(path) for checkpoint, path in paths.items()}

    for checkpoint in CHECKPOINTS:
        capture = captures[checkpoint]
        identity = identities[checkpoint]
        require_current_capture(capture, checkpoint, failures)
        require_required_planes_finite(capture, checkpoint, failures)
        require(
            capture.spacing_voxels == spacing_voxels,
            f"{checkpoint}: spacing is not {spacing_voxels}",
            failures,
        )
        require(
            identity.get("schema") == "re-flora-ddgi-radiance-capture-v1",
            f"{checkpoint}: wrong identity schema",
            failures,
        )
        require(
            identity.get("checkpoint") == checkpoint,
            f"{checkpoint}: sidecar checkpoint mismatch",
            failures,
        )
        require(
            field_matches_capture(identity["active_field"], capture),
            f"{checkpoint}: sidecar active field does not match v10 header",
            failures,
        )

    baseline = identities["baseline"]
    r2 = identities["r2-next-frame"]
    r4 = identities["r4-next-frame"]
    final = identities["final"]
    baseline_revision = baseline["active_field"]["radiance_revision"]

    for checkpoint, identity in (("r2-next-frame", r2), ("r4-next-frame", r4)):
        require(
            identity["capture_frame"] == identity["mutation_frame"] + 1,
            f"{checkpoint}: capture is not the first rendered frame after mutation",
            failures,
        )
        require(
            identity["active_field"] == baseline["active_field"],
            f"{checkpoint}: old consumer-visible DDGI field changed",
            failures,
        )

    baseline_sun = baseline["live_snapshot"]
    r2_sun = r2["live_snapshot"]
    r4_sun = r4["live_snapshot"]
    for checkpoint, changed in (("r2-next-frame", r2_sun), ("r4-next-frame", r4_sun)):
        for field in ("sun_direction", "sun_color", "sun_luminance"):
            require(
                changed[field] != baseline_sun[field],
                f"{checkpoint}: {field} did not dynamically change",
                failures,
            )

    require(
        r2["live_radiance_revision"] == baseline_revision + 1
        and r2["latest_radiance_revision"] == baseline_revision + 1,
        "r2-next-frame: live/latest revision is not r2",
        failures,
    )
    require(
        r2["building_field"]["radiance_revision"] == baseline_revision + 1
        and r2["builder_latched_radiance_revision"] == baseline_revision + 1
        and r2["builder_latched_snapshot"] == r2["live_snapshot"],
        "r2-next-frame: builder did not latch the exact r2 snapshot",
        failures,
    )
    require(
        r4["live_radiance_revision"] == baseline_revision + 3
        and r4["latest_radiance_revision"] == baseline_revision + 3,
        "r4-next-frame: latest-wins revision is not r4",
        failures,
    )
    require(
        r4["building_field"] == r2["building_field"]
        and r4["builder_latched_radiance_revision"] == baseline_revision + 1
        and r4["builder_latched_snapshot"] == r2["live_snapshot"],
        "r4-next-frame: in-flight r2 identity or latched snapshot mutated",
        failures,
    )
    final_active = final["active_field"]
    r2_building = r2["building_field"]
    require(
        final["live_radiance_revision"] == baseline_revision + 3
        and final["latest_radiance_revision"] == baseline_revision + 3
        and final_active["radiance_revision"] == baseline_revision + 3,
        "final: latest r4 is not consumer-active",
        failures,
    )
    require(
        final_active["source_field_serial"] == r2_building["field_serial"]
        and final_active["field_serial"] == r2_building["field_serial"] + 1,
        "final: r3 allocated a field or r4 did not consume r2",
        failures,
    )

    frame_comparisons = {}
    direct_comparisons = {}
    for checkpoint in ("r2-next-frame", "r4-next-frame"):
        frame = analyzer.compare_radiance_frame(
            captures[checkpoint], captures["baseline"]
        )
        direct = analyzer.compare_direct_light_baseline(
            captures[checkpoint], captures["baseline"], sunlit_roi
        )
        frame_comparisons[checkpoint] = frame
        direct_comparisons[checkpoint] = direct
        require(frame["compatible"], f"{checkpoint}: field metadata changed", failures)
        require(
            frame["environment_payload_bit_exact"],
            f"{checkpoint}: old DDGI irradiance payload changed",
            failures,
        )
        require(
            frame["world_xyz_bit_exact"] and frame["terrain_hit_mask_bit_exact"],
            f"{checkpoint}: world XYZ or terrain hit mask changed",
            failures,
        )
        require(
            direct["compatible"]
            and direct["sunlit_roi_luminance_absolute_delta"] >= min_direct_delta,
            f"{checkpoint}: direct-light ROI delta is below {min_direct_delta:g}",
            failures,
        )

    return {
        "base_capture": str(base),
        "spacing_voxels": spacing_voxels,
        "sunlit_roi": list(sunlit_roi),
        "min_direct_delta": min_direct_delta,
        "frame_comparisons": frame_comparisons,
        "direct_comparisons": direct_comparisons,
        "identities": identities,
        "validation_failures": failures,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("capture", type=Path, help="final radiance .rfirr path")
    parser.add_argument("--expect-spacing-voxels", type=int, required=True)
    parser.add_argument("--direct-light-sunlit-roi", type=float, nargs=6, required=True)
    parser.add_argument("--min-direct-light-roi-delta", type=float, required=True)
    args = parser.parse_args()
    report = validate(
        args.capture,
        args.expect_spacing_voxels,
        tuple(args.direct_light_sunlit_roi),
        args.min_direct_light_roi_delta,
    )
    print(json.dumps(report, indent=2, sort_keys=True))
    return 1 if report["validation_failures"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
