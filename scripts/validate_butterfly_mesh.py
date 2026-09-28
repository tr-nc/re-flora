#!/usr/bin/env python3
"""Run the real renderer's fixed-resolution butterfly acceptance fixture.

Example: python3 scripts/validate_butterfly_mesh.py --seconds 12
Requires a Vulkan-capable display/session, even though the window is hidden.
This is an explicit GPU check, not part of cargo test and not a perf acceptance.
"""
import argparse
import hashlib
import os
import re
import subprocess
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--seconds', type=float, default=12, help='App runtime; increase on slow GPUs if the seven-stage sweep does not finish (default: 12)')
    args = parser.parse_args()
    if args.seconds <= 0:
        parser.error('--seconds must be positive')
    root = Path(__file__).resolve().parent.parent
    os.chdir(root)
    output = root / 'target/butterfly-resume'
    output.mkdir(parents=True, exist_ok=True)
    log = output / 'renderer-validation.log'
    config = root / 'config/gui.toml'
    before = hashlib.sha256(config.read_bytes()).digest()
    env = dict(os.environ, RE_FLORA_BUTTERFLY_MESH_REVIEW='sweep', RE_FLORA_MODEL_CACHE_REVIEW='1')
    with log.open('w') as stream:
        result = subprocess.run(['cargo', 'run', '--release', '--', '--hidden', '--mute', '--perf',
            '--screenshot', 'player-default', str(output/'renderer-validation.png'),
            '--screenshot-delay', str(args.seconds*.75), '--auto-exit', str(args.seconds)],
            env=env, stdout=stream, stderr=subprocess.STDOUT, check=False)
    text = log.read_text()
    assert result.returncode == 0, f'App/build failed; inspect {log}'
    assert not re.search(r'\bERROR\b|VUID-|panicked at', text), f'Runtime/validation error; inspect {log}'
    assert before == hashlib.sha256(config.read_bytes()).digest(), 'Diagnostic changed saved GUI settings'
    for token in ['tile=8x8 fps=2', 'tile=22x22 fps=60', 'tile=24x24 fps=60',
                  'transmission=0.5', 'transmission=1', 'active=21', 'butterfly.tiles=', 'failures=0']:
        assert token in text, f'Missing {token!r}; inspect {log}; increase --seconds if sweep incomplete'
    assert not re.search(r'MODEL_CACHE_BAKE_CHECK[^\n]*mismatches=[1-9]', text)
    assert re.search(r'MODEL_CACHE_CONSUMED\] leaf=\d+ apple=\d+ butterfly=[1-9]\d*', text)
    for n in [8, 22, 24]:
        assert re.search(rf'MODEL_CACHE_GEOMETRY_CHECK\] kind=2 resolution={n} views=32 cases=64 checked_hits=[1-9]\d*', text), f'Missing independent bake geometry check for {n}px'
    assert 'rotating_pixels=true' in text, 'Missing rotating pixel display'
    print(f'PASS: cache-only 8/22/24px, transmission sweeps, independent bake coverage/depth (64 cases per resolution), no saved-setting changes.\nLog: {log}\nScreenshot: {output / "renderer-validation.png"}')



if __name__ == '__main__':
    main()
