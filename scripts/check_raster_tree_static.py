#!/usr/bin/env python3
"""Capture normal/pixelized display of the same live procedural wood mesh.

Build with cargo build --release first. A is normal mesh display; B is current-
camera pixelization, never the removed voxel tree or a cached viewing angle.
The GPU lock covers all config/camera changes, restored on success or failure.
This is a visual diagnostic, not performance or manual appearance acceptance.
"""
import argparse
import fcntl
import json
import math
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CAMERA = '''
[[snapshots]]
name = "raster-tree-review"
description = "Current-camera mesh display comparison"
position = [1.0, 0.64, 1.55]
yaw_deg = 0.0
pitch_deg = 0.0
fov_deg = 55.0
fly_mode = true
'''


def setting(source, name, value):
    pattern = rf'(id = "{re.escape(name)}".*?\[section.param.data\]\nvalue = )[^\n]+'
    result, count = re.subn(pattern, lambda match: match[1] + value, source, count=1, flags=re.DOTALL)
    if count != 1:
        raise ValueError(f"missing GUI parameter: {name}")
    return result


def mesh_geometry_evidence(text):
    meshes = re.findall(r'\[TREE\]\[MESH\] trees=(\d+) vertices=(\d+) triangles=(\d+) '
                        r'compile_ms=[\d.]+ rest_fingerprint=([0-9a-f]+) terrain_voxel_writes=0', text)
    if not meshes:
        raise ValueError('missing continuous mesh evidence; rebuild with cargo build --release')
    trees, vertices, triangles, fingerprint = meshes[-1]
    if min(int(trees), int(vertices), int(triangles)) <= 0:
        raise ValueError('capture requires a nonempty wood mesh')
    return {'trees': int(trees), 'vertices': int(vertices), 'triangles': int(triangles),
            'rest_fingerprint': fingerprint}


def thin_geometry_evidence(text):
    evidence = mesh_geometry_evidence(text)
    cones = re.findall(r'\[TREE\]\[THIN_WOOD\] authored=true radius_min=([\d.]+) '
                       r'subhalf_voxel_cones=(\d+)', text)
    if not cones:
        raise ValueError('missing actual thin branch evidence')
    radius, subhalf = cones[-1]
    if not (0.0 <= float(radius) < 0.5 and int(subhalf) > 0):
        raise ValueError('scene does not exercise actual thin authored branches')
    return {**evidence, 'minimum_radius_voxels': float(radius), 'subhalf_voxel_cones': int(subhalf)}


def main():
    parser = argparse.ArgumentParser(description=__doc__, epilog=(
        'Use --wind to animate the same mesh in both modes. The old --axis-aligned '
        'and --hybrid-lighting modes are removed. Example: '
        'python3 scripts/check_raster_tree_static.py --pixel-size 4 --thin-branches'))
    parser.add_argument('--output', type=Path, default=ROOT / 'target/tree-display-evidence')
    parser.add_argument('--wind', action='store_true', help='Animate BOTH displays with scripted gusts')
    parser.add_argument('--pixel-size', type=int, choices=range(1, 17), metavar='1..16', default=4,
                        help='scene pixels per B cell (default: 4); A is unchanged')
    parser.add_argument('--thin-branches', action='store_true',
                        help='Also verify authored branch radii below half a voxel; no voxelization')
    parser.add_argument('--time-of-day', type=float, default=0.47,
                        help='fixed lighting time for both captures (0..1; default: 0.47)')
    parser.add_argument('--delay', type=float, default=4.0, help='capture delay in seconds (default: 4)')
    args = parser.parse_args()
    if not 0.0 <= args.time_of_day <= 1.0:
        parser.error('--time-of-day must be between 0 and 1')
    if not math.isfinite(args.delay) or args.delay <= 0:
        parser.error('--delay must be finite and positive')
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.pop('WAYLAND_DISPLAY', None)
    if args.wind:
        env['RE_FLORA_WIND_PROTOTYPE_SMOKE'] = '1'
    gui = ROOT / 'config/gui.toml'
    camera = ROOT / 'config/camera_snapshots.toml'
    with open('/tmp/re-flora-summer-gpu.lock', 'w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        gui_original, camera_original = gui.read_bytes(), camera.read_bytes()
        try:
            source = setting(gui_original.decode(), 'auto_daynight_cycle', 'false')
            source = setting(source, 'time_of_day', str(args.time_of_day))
            source = setting(source, 'path_tracing_reference', 'false')
            source = setting(source, 'tree_wind', str(args.wind).lower())
            source = setting(source, 'tree_pixel_size', str(args.pixel_size))
            evidence = {}
            (out / 'geometry.json').unlink(missing_ok=True)
            camera.write_text(CAMERA)
            for foliage in [False, True]:
                for mode in ['A', 'B']:
                    gui.write_text(setting(source, 'tree_pixelized', str(mode == 'B').lower()))
                    name = f'{mode}-' + ('canopy' if foliage else 'wood')
                    (out / f'{name}.png').unlink(missing_ok=True)
                    cmd = [str(ROOT / 'target/release/re-flora'), '--hidden', '--mute',
                           '--no-particles', '--no-clouds', '--no-god-rays', '--no-lens-flare',
                           '--screenshot', 'raster-tree-review', str(out / f'{name}.png'),
                           '--screenshot-delay', str(args.delay), '--auto-exit', str(args.delay + 2.0)]
                    if not foliage:
                        cmd.append('--no-flora')
                    with (out / f'{name}.log').open('w') as log:
                        subprocess.run(cmd, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT,
                                       check=True, timeout=max(90, args.delay + 30))
                    assert (out / f'{name}.png').is_file(), f'{name}: screenshot missing'
                    text = (out / f'{name}.log').read_text()
                    assert 'Application exited successfully' in text, name
                    assert not any(error in text for error in [' ERROR ', 'VUID-', 'panicked at']), name
                    evidence[name] = (thin_geometry_evidence if args.thin_branches else mesh_geometry_evidence)(text)
                    if mode == 'B' and evidence[name] != evidence[name.replace('B-', 'A-', 1)]:
                        raise ValueError(f'{name}: display A/B changed the procedural geometry')
                    print(out / f'{name}.png', flush=True)
            (out / 'geometry.json').write_text(json.dumps(evidence, indent=2) + '\n')
        finally:
            gui.write_bytes(gui_original)
            camera.write_bytes(camera_original)


if __name__ == '__main__':
    main()
