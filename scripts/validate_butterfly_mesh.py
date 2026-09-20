#!/usr/bin/env python3
"""Run the real renderer's fixed-resolution butterfly acceptance fixture.

Example: python3 scripts/validate_butterfly_mesh.py --seconds 12
Requires a Vulkan-capable display/session, even though the window is hidden.
This is an explicit GPU check, not part of cargo test and not a perf acceptance.
"""
import argparse
import hashlib
import os
from pathlib import Path
import re
import struct
import subprocess


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
    for image in (output/'game-tiles').glob('*px-*-shadow*-*.png'):
        image.unlink()  # Never accept native tiles left over from a previous run.
    env = dict(os.environ, RE_FLORA_BUTTERFLY_MESH_REVIEW='sweep')
    with log.open('w') as stream:
        result = subprocess.run(['cargo', 'run', '--release', '--', '--hidden', '--mute', '--perf',
            '--screenshot', 'player-default', str(output/'renderer-validation.png'),
            '--screenshot-delay', str(args.seconds*.75), '--auto-exit', str(args.seconds)],
            env=env, stdout=stream, stderr=subprocess.STDOUT)
    text = log.read_text()
    assert result.returncode == 0, f'App/build failed; inspect {log}'
    assert not re.search(r'\bERROR\b|VUID-|panicked at', text), f'Runtime/validation error; inspect {log}'
    assert before == hashlib.sha256(config.read_bytes()).digest(), 'Diagnostic changed saved GUI settings'
    for token in ['tile=8x8 fps=2', 'tile=22x22 fps=60', 'tile=64x64 fps=60',
                  'self_shadows=false', 'transmission=0.5', 'transmission=1', 'active=21', 'butterfly.tiles=', 'failures=0']:
        assert token in text, f'Missing {token!r}; inspect {log}; increase --seconds if sweep incomplete'
    for n in [8,22,64]:
        match = re.search(rf'BUTTERFLY-MESH-CHECK\] tile={n}x{n} active=21 checked_hits=(\d+)',text)
        assert match and int(match[1]) > 0, f'Missing real GPU/CPU hit-depth check for {n}px'
        images = list((output/'game-tiles').glob(f'{n}px-*-shadow1-trans0-*.png'))
        assert len(images) == 21, f'Expected 21 native {n}px tiles, found {len(images)}'
        for image in images:
            dimensions = struct.unpack('>II', image.read_bytes()[16:24])
            assert dimensions == (n,n), (image,dimensions)
    for transmission in [50, 100]:
        images = list((output/'game-tiles').glob(f'22px-60fps-shadow1-trans{transmission}-*.png'))
        assert len(images) == 21, f'Missing transmission fixture {transmission}%'
    print(f'PASS: real 8/22/64px tiles, CPU hit-depth oracle and live self-shadow switches; no saved-setting changes.\nLog: {log}\nScreenshot: {output / "renderer-validation.png"}')


if __name__ == '__main__':
    main()
