#!/usr/bin/env python3
"""Capture static tree A/B with real shadows; restores the worktree's GUI/camera files.

Build with cargo build --release first. The GPU lock covers all configuration changes
and captures. This is a visual diagnostic, not a performance or image-equivalence test.
"""
from pathlib import Path
import argparse
import fcntl
import json
import os
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
CAMERA = '''
[[snapshots]]
name = "raster-tree-review"
description = "Static raster tree comparison"
position = [1.0, 0.64, 1.55]
yaw_deg = 0.0
pitch_deg = 0.0
fov_deg = 55.0
fly_mode = true
'''

def setting(source, name, value):
    pattern = rf'(id = "{re.escape(name)}".*?\[section.param.data\]\nvalue = )[^\n]+'
    result, count = re.subn(pattern, lambda match: match[1] + value, source, count=1, flags=re.S)
    if count != 1:
        raise ValueError(f"missing GUI parameter: {name}")
    return result


def tree_setting(source, name, value):
    pattern = r'(\[tree\.desc\]\n)(.*?)(?=^\[|\Z)'

    def replace(match):
        body, count = re.subn(rf'^{re.escape(name)} = [^\n]+$', f'{name} = {value}',
                              match[2], flags=re.MULTILINE)
        if count != 1:
            raise ValueError(f'missing tree parameter: {name}')
        return match[1] + body

    result, count = re.subn(pattern, replace, source, count=1, flags=re.MULTILINE | re.DOTALL)
    if count != 1:
        raise ValueError('missing [tree.desc]')
    return result


def thin_geometry_evidence(text):
    cones = re.findall(r'\[TREE\]\[THIN_WOOD\] preserve=true cull=\w+ radius_min=([\d.]+) '
                       r'subminimum_cones=(\d+) subhalf_voxel_cones=(\d+)', text)
    meshes = re.findall(r'\[TREE\]\[NORMAL_CONFIDENCE\] fallback=(\d+) transition=(\d+) reliable=(\d+) '
                        r'single_voxel_cross_sections=(\d+) rest_fingerprint=([0-9a-f]+)', text)
    if not cones or not meshes:
        raise ValueError('missing actual thin cone/mesh evidence; rebuild the release binary')
    radius, subminimum, subhalf = cones[-1]
    fallback, transition, reliable, single_voxel, fingerprint = meshes[-1]
    if not (0.0 <= float(radius) < 0.5 and int(subminimum) > 0 and int(subhalf) > 0
            and int(fallback) > 0 and int(single_voxel) > 0):
        raise ValueError('scene does not exercise actual thin wood and degenerate voxel normals')
    return {'minimum_radius_voxels': float(radius), 'subminimum_cones': int(subminimum),
            'subhalf_voxel_cones': int(subhalf), 'fallback_cells': int(fallback),
            'transition_cells': int(transition), 'reliable_cells': int(reliable),
            'single_voxel_cross_sections': int(single_voxel), 'rest_fingerprint': fingerprint}


def main():
    parser = argparse.ArgumentParser(description=__doc__, epilog=(
        'The experimental --axis-aligned mode was removed. '
        'Use --wind to capture the retained smooth animation.'))
    parser.add_argument('--output', type=Path, default=ROOT / 'target/raster-tree-evidence')
    parser.add_argument('--wind', action='store_true', help='Capture B with tree wind and scripted gusts')
    parser.add_argument('--hybrid-lighting', action='store_true',
                        help='Keep raster trees in A/B; compare original vs hybrid thin-branch lighting')
    parser.add_argument('--thin-branches', action='store_true',
                        help='With --hybrid-lighting, preserve authored thin radii in BOTH modes; verify identical meshes')
    parser.add_argument('--time-of-day', type=float, default=0.47,
                        help='Fixed lighting time for both captures (0..1; default: 0.47)')
    parser.add_argument('--delay', type=float, default=4.0)
    args = parser.parse_args()
    if args.thin_branches and not args.hybrid_lighting:
        parser.error('--thin-branches requires --hybrid-lighting so A/B compares lighting on identical thin geometry')
    if not 0.0 <= args.time_of_day <= 1.0:
        parser.error('--time-of-day must be between 0 and 1')
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
            source = setting(source, 'raster_tree_wind', str(args.wind).lower())
            if args.thin_branches:
                source = tree_setting(source, 'preserve_thin_branches', 'true')
            evidence = {}
            if args.thin_branches:
                (out / 'thin-geometry.json').unlink(missing_ok=True)
            camera.write_text(CAMERA)
            for foliage in [False, True]:
                for mode in ['A', 'B']:
                    candidate = setting(source, 'raster_tree_static',
                                        str(args.hybrid_lighting or mode == 'B').lower())
                    candidate = setting(candidate, 'raster_tree_hybrid_lighting',
                                        str(args.hybrid_lighting and mode == 'B').lower())
                    gui.write_text(candidate)
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
                                       check=True, timeout=90)
                    assert (out / f'{name}.png').is_file(), f'{name}: screenshot missing'
                    text = (out / f'{name}.log').read_text()
                    assert 'Application exited successfully' in text, name
                    if mode == 'B' or args.hybrid_lighting:
                        assert '[TREE][RASTER_STATIC] mode=B' in text, name
                    assert not any(error in text for error in [' ERROR ', 'VUID-', 'panicked at']), name
                    if args.thin_branches:
                        evidence[name] = thin_geometry_evidence(text)
                        if mode == 'B' and evidence[name] != evidence[name.replace('B-', 'A-', 1)]:
                            raise ValueError(f'{name}: lighting A/B changed the thin geometry or normal metadata')
                    print(out / f'{name}.png', flush=True)
            if args.thin_branches:
                (out / 'thin-geometry.json').write_text(json.dumps(evidence, indent=2) + '\n')
        finally:
            gui.write_bytes(gui_original)
            camera.write_bytes(camera_original)

if __name__ == '__main__':
    main()
