#!/usr/bin/env python3
"""Capture the seed-122 tree and detect its exposed, completely black branch voxels.

Build with CARGO_BUILD_JOBS=2 cargo build --release first. This is a real Vulkan
app regression, deliberately separate from ordinary unit tests. No light floor,
shadow disable, or display-RGB threshold is used.
"""

from __future__ import annotations

import argparse
import json
import math
import os
from pathlib import Path
import statistics
import subprocess

from analyze_environment_irradiance_capture import PIXEL, load_capture, shadow_source_channels

ROOT = Path(__file__).resolve().parents[1]
CAMERA = """
[[snapshots]]
name = "tree-branch-regression"
description = "Seed-122 tree branch lighting regression"
position = [1.0, 0.64, 1.55]
yaw_deg = 0.0
pitch_deg = 0.0
fov_deg = 55.0
fly_mode = true
"""
BLACKY_CAMERA = """
[[snapshots]]
name = "tree-blacky-regression"
description = "User Blacky snapshot: near-black trunk side despite nonzero irradiance"
position = [0.816733, 0.6137017, 0.8741581]
yaw_deg = 101.928986
pitch_deg = 12.935698
fov_deg = 60.0
fly_mode = true
"""
BLACKY_PATCH_VOXELS = (
    (265, 169, 230), (275, 183, 225), (268, 176, 223), (268, 175, 224),
    (265, 170, 229), (265, 170, 230), (265, 169, 229), (265, 168, 230),
    (264, 165, 230), (264, 164, 230),
)
BLACKY_NEIGHBOR_VOXEL = (264, 169, 231)


def measure_blacky(path: Path) -> dict:
    """Track the actual central black patch, not unrelated exact-zero pixels.

    This fixed-scene contrast guard is deliberately not a general rule that
    shadowed pixels must be bright. It observes the user's identified patch and
    its neighboring trunk voxel through the production irradiance capture.
    """
    capture = load_capture(path)
    samples = {voxel: [] for voxel in (*BLACKY_PATCH_VOXELS, BLACKY_NEIGHBOR_VOXEL)}
    nonfinite = 0
    for environment, receiver in zip(
        PIXEL.iter_unpack(capture.payload),
        PIXEL.iter_unpack(capture.terrain_shadow_receiver_payload), strict=True,
    ):
        if not environment[3]:
            continue
        if not all(math.isfinite(v) for v in (*environment, *receiver)):
            nonfinite += 1
            continue
        voxel = tuple(round(v * 256 - .5) for v in receiver[:3])
        if voxel in samples:
            samples[voxel].append(sum(environment[:3]))
    patch = samples[BLACKY_PATCH_VOXELS[0]]
    neighbor = samples[BLACKY_NEIGHBOR_VOXEL]
    patch_energy = statistics.median(patch) if patch else 0.0
    neighbor_energy = statistics.median(neighbor) if neighbor else 0.0
    ratio = patch_energy / neighbor_energy if neighbor_energy > 0 else 0.0
    patch_energies = {
        str(voxel): statistics.median(samples[voxel]) if samples[voxel] else 0.0
        for voxel in BLACKY_PATCH_VOXELS
    }
    dark_patches = sum(energy < neighbor_energy * .25 for energy in patch_energies.values())
    valid = capture.plane_count == 5 and min(map(len, samples.values())) >= 3
    valid = valid and min(len(patch), len(neighbor)) >= 10
    valid = valid and neighbor_energy > .2 and not nonfinite
    return dict(
        verdict="INVALID" if not valid else "RED" if dark_patches else "GREEN",
        scene="blacky", capture=str(path), width=capture.width, height=capture.height,
        patch_samples=len(patch), neighbor_samples=len(neighbor),
        patch_median_rgb_sum=patch_energy, neighbor_median_rgb_sum=neighbor_energy,
        patch_to_neighbor_ratio=ratio, nonfinite=nonfinite,
        dark_patch_count=dark_patches, patch_median_rgb_sums=patch_energies,
    )


def measure(path: Path) -> dict:
    capture = load_capture(path)
    samples = 0
    unoccluded = 0
    black = 0
    black_voxels = set()
    nonfinite = 0
    for environment, world, direct, receiver, shadow in zip(
        *(PIXEL.iter_unpack(plane) for plane in (
            capture.payload, capture.world_payload, capture.direct_light_payload,
            capture.terrain_shadow_receiver_payload, capture.direct_sun_shadow_payload,
        )), strict=True,
    ):
        # Above the terrain, inside the known startup-tree crown. Leaves/raster
        # objects are excluded by the runner; these are actual terrain hits.
        if not (environment[3] and .85 < world[0] < 1.18
                and .5 < world[1] < .85 and .7 < world[2] < 1.3):
            continue
        samples += 1
        if not all(math.isfinite(v) for plane in
                   (environment, world, direct, receiver, shadow) for v in plane):
            nonfinite += 1
            continue
        if world[3] < .999 or min(shadow[channel] for _, channel in shadow_source_channels(capture.version)) < .999:
            continue
        unoccluded += 1
        if max(environment[:3]) + max(direct[:3]) <= 1e-6:
            black += 1
            black_voxels.add(tuple(round(v * 256 - .5) for v in receiver[:3]))
    valid = capture.plane_count == 5 and samples > 10000 and unoccluded > 1000
    return dict(
        verdict="INVALID" if not valid or nonfinite else "RED" if black else "GREEN",
        capture=str(path), width=capture.width, height=capture.height,
        tree_samples=samples, unoccluded_samples=unoccluded,
        zero_energy_samples=black, zero_energy_voxels=sorted(black_voxels),
        nonfinite=nonfinite,
    )


def run(output: Path, screenshot: bool, binary: Path | None = None,
        scene: str = "startup") -> Path:
    output.mkdir(parents=True, exist_ok=True)
    gui = ROOT / "config/gui.toml"
    camera = ROOT / "config/camera_snapshots.toml"
    originals = {path: path.read_bytes() for path in (gui, camera)}
    capture = output / "light.rfirr"
    capture.unlink(missing_ok=True)
    command = [
        "flock", "/tmp/re-flora-summer-gpu.lock", str(binary or ROOT / "target/release/re-flora"),
        "--hidden", "--mute", "--windowed", "--no-particles",
        "--no-god-rays", "--no-lens-flare", "--auto-exit", "12",
    ]
    if scene == "startup":
        command.append("--no-flora")
    camera_name = "tree-blacky-regression" if scene == "blacky" else "tree-branch-regression"
    if screenshot:
        command += ["--screenshot", camera_name, str(output / "final.png"),
                    "--screenshot-delay", "3"]
    else:
        command += ["--camera-snapshot", camera_name,
                    "--environment-irradiance-capture", str(capture),
                    "--environment-irradiance-capture-target", "published"]
    try:
        preset = BLACKY_CAMERA if scene == "blacky" else CAMERA
        camera.write_bytes(originals[camera] + preset.encode())
        (output / "gui.toml").write_bytes(originals[gui])
        (output / "command.json").write_text(json.dumps(command, indent=2) + "\n")
        with (output / "stdout.log").open("w") as log:
            subprocess.run(command, cwd=ROOT, env=dict(os.environ, CARGO_BUILD_JOBS="2"),
                           stdout=log, stderr=subprocess.STDOUT, check=True, timeout=120)
        logs = (output / "stdout.log").read_text()
        if "Application exited successfully" not in logs or " ERROR " in logs:
            raise RuntimeError("app did not exit successfully; inspect stdout.log")
        for flag, name in (("--latest-log", "latest-log.txt"),
                           ("--tail-latest-log", "tail.log")):
            result = subprocess.run(command[:3] + [flag], cwd=ROOT, capture_output=True,
                                    text=True, check=True, timeout=120)
            (output / name).write_text(result.stdout + result.stderr)
    finally:
        for path, contents in originals.items():
            if path.read_bytes() != contents:
                path.write_bytes(contents)
    expected = output / "final.png" if screenshot else capture
    if not expected.is_file():
        raise RuntimeError(f"missing app artifact: {expected}")
    return capture


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, help="explicit release binary for baseline comparisons")
    parser.add_argument("--capture", type=Path, help="analyze an existing capture")
    parser.add_argument("--output", type=Path, default=ROOT / "target/summer-evidence/tree-branch")
    parser.add_argument("--screenshot", action="store_true", help="separate real-game image run")
    parser.add_argument("--scene", choices=("startup", "blacky"), default="startup")
    args = parser.parse_args()
    path = args.capture or run(args.output.resolve(), args.screenshot,
                              args.binary.resolve() if args.binary else None, args.scene)
    if args.screenshot:
        return 0
    result = measure_blacky(path) if args.scene == "blacky" else measure(path)
    print("[TREE_BRANCH_LIGHTING] " + json.dumps(result, sort_keys=True))
    if not args.capture:
        (args.output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return {"GREEN": 0, "RED": 1, "INVALID": 2}[result["verdict"]]


if __name__ == "__main__":
    raise SystemExit(main())
