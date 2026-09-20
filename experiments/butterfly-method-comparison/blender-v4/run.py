#!/usr/bin/env python3
"""Export saved wing-only v4 source. --rebuild explicitly replaces it from the recipe.

Requires Blender 4.5 LTS and system Python + Pillow. Outputs stay in this folder.
--draft skips independent replay; never counts as a complete validation.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('v2_palette', ROOT.parent / 'blender-v2/run.py')
palette = importlib.util.module_from_spec(spec)
spec.loader.exec_module(palette)
ANGLES = (0, 45, 90, 135, 180)
SIZES = (12, 16, 128)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def assemble(draft):
    metrics = []
    replay_count = 0
    contact = Image.new('RGB', (610, 1060), '#253039')
    draw = ImageDraw.Draw(contact)
    for size in SIZES:
        sheet = Image.new('RGBA', (size * 5, size * 5))
        for row, angle in enumerate(ANGLES):
            cells = []
            for col in range(6):
                relative = Path(f'native{size}/view{angle:03d}_frame{col:02d}.png')
                with Image.open(ROOT / 'raw' / relative) as im:
                    cell = im.convert('RGBA')
                assert cell.size == (size, size), relative
                if not draft:
                    with Image.open(ROOT / 'replay' / relative) as im:
                        assert cell.tobytes() == im.convert('RGBA').tobytes(), relative
                    replay_count += 1
                cells.append(cell)
            assert cells[0].tobytes() == cells[5].tobytes(), (size, angle, 'loop')
            for col, cell in enumerate(cells[:5]):
                sheet.paste(cell, (col * size, row * size))
                a = cell.getchannel('A')
                clear = all(not a.getpixel((x, y)) for x in range(size) for y in range(size)
                            if x in (0, size-1) or y in (0, size-1))
                assert clear, (size, angle, col, 'boundary')
                if size == 128:
                    continue
                indexed = palette.indexed(cell, palette.BLUE).convert('RGBA')
                alpha = indexed.getchannel('A')
                metrics.append({'size': size, 'angle': angle, 'frame': col,
                                'opaque_pixels': sum(v > 0 for v in alpha.getdata()),
                                'bbox': alpha.getbbox()})
                y = (0 if size == 12 else 530) + 30 + row * 98
                zoom = indexed.resize((96, 96), Image.Resampling.NEAREST)
                contact.paste(zoom, (130 + col * 96, y), zoom)
                draw.text((8, y), f'{size}px / {angle} deg', fill='white')
        sheet.save(ROOT / f'atlas-{size}-rgba.png')
        if size < 128:
            for name, colors in (('blue', palette.BLUE), ('gray', palette.GRAY)):
                image = sheet if name == 'blue' else Image.merge('RGBA', (*[sheet.convert('L')]*3, sheet.getchannel('A')))
                path = ROOT / f'atlas-{size}-{name}-indexed.png'
                palette.save_indexed(palette.indexed(image, colors), path)
                with Image.open(path) as im:
                    assert im.mode == 'P' and len(im.getpalette()) == 15
                    assert set(im.convert('RGBA').getchannel('A').getdata()) == {0, 255}
    for i, size in enumerate((12, 16)):
        draw.text((8, i*530+8), f'V4 wing-only native {size}px - diagnostic samples', fill='white')
    contact.save(ROOT / 'contact.png')
    data = (ROOT / 'butterfly-prototype.glb').read_bytes()
    length = struct.unpack('<I', data[12:16])[0]
    glb = json.loads(data[20:20+length])
    assert len(glb['animations']) == 1
    clip = glb['animations'][0]
    channels = [(glb['nodes'][c['target']['node']]['name'], c['target']['path']) for c in clip['channels']]
    assert set(channels) == {('Flight pose', 'translation'), ('Flight pose', 'rotation'),
                             ('Wing hinge L', 'rotation'), ('Wing hinge R', 'rotation')}
    mesh_nodes = [n['name'] for n in glb['nodes'] if 'mesh' in n]
    assert len(mesh_nodes) == 2 and all('wing' in n for n in mesh_nodes), mesh_nodes
    assert not any(word in n['name'].lower() for n in glb['nodes']
                   for word in ('abdomen', 'thorax', 'head', 'body')), 'Body node remains'
    for sampler in clip['samplers']:
        accessor = glb['accessors'][sampler['input']]
        assert accessor['min'] == [0] and accessor['max'] == [1]
    result = {'draft': draft, 'replay_frames': replay_count,
              'all_boundaries_clear': True, 'loop_checks': 15, 'glb_channels': channels,
              'mesh_nodes': mesh_nodes, 'body_geometry_absent': True,
              'metrics': metrics, 'sha256': {p.name: digest(p) for p in
                  [ROOT/'butterfly-prototype.blend', ROOT/'butterfly-prototype.glb',
                   ROOT/'create_export_blender.py', ROOT/'run.py', *sorted(ROOT.glob('atlas-*.png'))]}}
    (ROOT/'validation.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps({k: v for k, v in result.items() if k not in ('metrics', 'sha256')}, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rebuild', action='store_true')
    parser.add_argument('--draft', action='store_true')
    args = parser.parse_args()
    blender = os.environ.get('BLENDER') or shutil.which('blender') or str(Path.home()/'.local/opt/blender-4.5.13-linux-x64/blender')
    def run(mode, folder, existing=True):
        command = [blender, '--background', '--factory-startup']
        if existing:
            command.append(str(ROOT/'butterfly-prototype.blend'))
        command += ['--python', str(ROOT/'create_export_blender.py'), '--', '--mode', mode, '--output', str(folder)]
        with (ROOT/f'{mode}-{folder.name}.log').open('w') as log:
            subprocess.run(command, check=True, stdout=log, stderr=subprocess.STDOUT)
    if args.rebuild:
        run('create', ROOT, False)
    elif not (ROOT/'butterfly-prototype.blend').exists():
        parser.error('Saved model missing; use --rebuild to create it from the recipe')
    before = digest(ROOT/'butterfly-prototype.blend')
    run('export-glb', ROOT)
    run('render', ROOT/'raw')
    if not args.draft:
        run('render', ROOT/'replay')
    assert digest(ROOT/'butterfly-prototype.blend') == before, 'Export changed saved source'
    assemble(args.draft)


if __name__ == '__main__':
    main()
