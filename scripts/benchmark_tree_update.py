#!/usr/bin/env python3
"""Compare static and smooth wind-driven release trees; restore GUI/camera bytes.

Build first with cargo build --release. Static disables wind on the wood surface;
smooth animates the same exposed-face topology. This is a whole-path single-tree
diagnostic, not an isolated animation kernel or a forest performance budget.
The experimental blocks mode has been removed; use smooth for animated trees.
"""
import argparse
import fcntl
import json
import math
import os
import re
import statistics
import subprocess
import sys
from pathlib import Path

from check_raster_tree_static import setting

ROOT = Path(__file__).resolve().parents[1]
MODES = {'static': False, 'smooth': True}
FRAME = re.compile(r'\[(\d+):(\d+):(\d+\.\d+) INFO .*?\[PERF\]\[FRAME\] frame (\d+) total ([\d.]+)ms')
UPDATE = re.compile(r'\[PERF\]\[TREE_UPDATE\] frame=(\d+) (.*)')
METRIC = re.compile(r'(\w+_us)=(?:Some\()?([\d.]+)')
GPU = re.compile(r'\[PERF\]\[GPU_FRAME_SCOPE\] frame (\d+) (.*)')
GPU_METRIC = re.compile(r'(tree_surface_update\.pass|tree_skin\.pass|frame\.render)=(\d+)us')


class Parser(argparse.ArgumentParser):
    def error(self, message):
        self.exit_with_help(message)

    def exit_with_help(self, message):
        print(f'error: {message}', file=sys.stderr)
        self.print_help(sys.stderr)
        self.exit(2)


def build_parser():
    parser = Parser(description=__doc__, epilog=(
        'Example: python3 scripts/benchmark_tree_update.py --output target/tree-review '
        '--modes static smooth --repeats 3 --seconds 12'))
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/re-flora',
                        help='release executable; can be a saved pre-change binary')
    parser.add_argument('--output', type=Path, required=True, help='directory for logs and summary.json')
    parser.add_argument('--seconds', type=float, default=8., help='run duration per mode (default: 8)')
    parser.add_argument('--warmup-seconds', type=float, default=3.,
                        help='exclude initial wall-clock seconds after the first frame log (default: 3)')
    parser.add_argument('--modes', nargs='+', choices=MODES, default=['static', 'smooth'],
                        help='modes to measure (default: static smooth)')
    parser.add_argument('--repeats', type=int, default=1,
                        help='runs per mode; rotates mode order each repeat (default: 1)')
    return parser


def samples_from_log(text, warmup_seconds):
    frames, updates, gpu = {}, {}, {}
    first_time = None
    previous_time = 0.
    day_offset = 0.
    for line in text.splitlines():
        if match := FRAME.search(line):
            hours, minutes, seconds, frame, ms = match.groups()
            clock = int(hours)*3600 + int(minutes)*60 + float(seconds)
            if clock < previous_time:
                day_offset += 86400.
            previous_time = clock
            timestamp = clock + day_offset
            if first_time is None:
                first_time = timestamp
            if timestamp - first_time >= warmup_seconds:
                frames[int(frame)] = float(ms)
        if match := UPDATE.search(line):
            updates[int(match[1])] = {key: float(value) for key, value in METRIC.findall(match[2])}
        if match := GPU.search(line):
            gpu[int(match[1])] = {key: float(value) for key, value in GPU_METRIC.findall(match[2])}
    return [{'frame_ms': ms, 'tree_update_us': updates.get(frame, {}),
             'gpu_scope_us': gpu.get(frame, {})} for frame, ms in frames.items()]


def percentiles(values):
    ordered = sorted(values)
    return {'samples': len(values), 'median': statistics.median(values),
            'p95': ordered[int((len(ordered)-1)*.95)]}


def summarize(samples):
    frame = percentiles([sample['frame_ms'] for sample in samples])
    result = {'samples': frame['samples'], 'frame_median_ms': frame['median'], 'frame_p95_ms': frame['p95']}
    for group in ['tree_update_us', 'gpu_scope_us']:
        keys = sorted({key for sample in samples for key in sample[group]})
        result[group] = {key: percentiles([sample[group][key] for sample in samples if key in sample[group]])
                         for key in keys}
    return result


def main():
    parser = build_parser()
    args = parser.parse_args()
    if (not math.isfinite(args.seconds) or not math.isfinite(args.warmup_seconds)
            or not 0 <= args.warmup_seconds < args.seconds):
        parser.error('require finite 0 <= --warmup-seconds < --seconds; leave enough time for 30 steady frames')
    if args.repeats < 1:
        parser.error('--repeats must be at least 1')
    if len(set(args.modes)) != len(args.modes):
        parser.error('--modes must not contain duplicates')
    if not args.binary.is_file():
        parser.error(f'release binary not found: {args.binary}; run cargo build --release or provide --binary')
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.pop('WAYLAND_DISPLAY', None)
    env['RE_FLORA_WIND_PROTOTYPE_SMOKE'] = '1'
    gui, camera = ROOT / 'config/gui.toml', ROOT / 'config/camera_snapshots.toml'
    samples = {name: [] for name in args.modes}
    runs = {name: [] for name in args.modes}
    with open('/tmp/re-flora-summer-gpu.lock', 'w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        original_gui, original_camera = gui.read_bytes(), camera.read_bytes()
        try:
            source = original_gui.decode()
            for name, value in [('raster_tree_static', 'true'), ('auto_daynight_cycle', 'false'), ('time_of_day', '0.47')]:
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
            for repeat in range(args.repeats):
                offset = repeat % len(args.modes)
                for name in args.modes[offset:] + args.modes[:offset]:
                    gui.write_text(setting(source, 'raster_tree_wind', str(MODES[name]).lower()))
                    stem = name if args.repeats == 1 else f'{name}-{repeat+1}'
                    log_path = out / f'{stem}.log'
                    with log_path.open('w') as log:
                        subprocess.run([str(args.binary.resolve()), '--hidden', '--mute', '--perf',
                                        '--no-particles', '--camera-snapshot', 'tree-update-bench',
                                        '--auto-exit', str(args.seconds)], cwd=ROOT, env=env,
                                       stdout=log, stderr=subprocess.STDOUT,
                                       timeout=max(120., args.seconds+90.), check=True)
                    text = log_path.read_text()
                    assert 'Application exited successfully' in text, log_path
                    assert '[TREE][RASTER_STATIC] mode=B' in text, log_path
                    assert not any(s in text for s in [' ERROR ', 'panicked at', 'VUID-']), log_path
                    values = samples_from_log(text, args.warmup_seconds)
                    assert len(values) >= 30, f'insufficient steady frames: {log_path}; increase --seconds'
                    samples[name].extend(values)
                    run = summarize(values)
                    runs[name].append(run)
                    print(stem, {k: v for k, v in run.items() if k not in ['tree_update_us', 'gpu_scope_us']}, flush=True)
        finally:
            gui.write_bytes(original_gui)
            camera.write_bytes(original_camera)
    summary = {name: {**summarize(values), 'runs': runs[name]} for name, values in samples.items()}
    (out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')


if __name__ == '__main__':
    main()
