#!/usr/bin/env python3
"""Release app comparison of both tree representations; restores GUI/camera bytes.

Build release first. --binary permits comparing a saved pre-change release executable.
This fixed single-tree scene is a diagnostic, not a forest performance budget.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
from check_raster_tree_static import setting

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/re-flora')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=float, default=8.)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.pop('WAYLAND_DISPLAY', None)
    env['RE_FLORA_WIND_PROTOTYPE_SMOKE'] = '1'
    gui, camera = ROOT / 'config/gui.toml', ROOT / 'config/camera_snapshots.toml'
    with open('/tmp/re-flora-summer-gpu.lock', 'w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        original_gui, original_camera = gui.read_bytes(), camera.read_bytes()
        summary = {}
        try:
            source = original_gui.decode()
            for name, value in [('raster_tree_static', 'true'), ('raster_tree_wind', 'true'),
                                ('auto_daynight_cycle', 'false'), ('time_of_day', '0.47')]:
                source = setting(source, name, value)
            camera.write_text('''[[snapshots]]
name = "tree-update-bench"
description = "Tree update benchmark"
position = [1.0, 0.67, 1.28]
yaw_deg = 0.0
pitch_deg = 0.0
fov_deg = 40.0
fly_mode = true
''')
            for name, aligned in [('smooth', False), ('blocks', True)]:
                gui.write_text(setting(source, 'raster_tree_axis_aligned', str(aligned).lower()))
                log_path = out / f'{name}.log'
                with log_path.open('w') as log:
                    subprocess.run([str(args.binary.resolve()), '--hidden', '--mute', '--perf',
                                    '--no-particles', '--camera-snapshot', 'tree-update-bench',
                                    '--auto-exit', str(args.seconds)], cwd=ROOT, env=env,
                                   stdout=log, stderr=subprocess.STDOUT, timeout=120, check=True)
                text = log_path.read_text()
                assert 'Application exited successfully' in text
                assert f'axis_aligned={str(aligned).lower()}' in text
                assert not any(s in text for s in [' ERROR ', 'panicked at', 'VUID-'])
                values = []
                elapsed = 0.
                for value in re.findall(r'\[PERF\]\[FRAME\] frame \d+ total ([\d.]+)ms', text):
                    ms = float(value)
                    elapsed += ms / 1000.
                    if elapsed >= 3.:
                        values.append(ms)
                assert len(values) >= 30, f'insufficient steady samples: {name}'
                ordered = sorted(values)
                summary[name] = dict(samples=len(values), frame_median_ms=statistics.median(values),
                                     frame_p95_ms=ordered[int((len(ordered)-1)*.95)])
                print(name, summary[name], flush=True)
        finally:
            gui.write_bytes(original_gui)
            camera.write_bytes(original_camera)
        (out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')


if __name__ == '__main__':
    main()
