"""Assemble the native-resolution diagnostic renders, not production assets.

/usr/bin/python3 THIS_FILE --input DIR --output DIR
Requires Pillow. All previews use the same 96px display box and nearest sampling.
"""
import argparse
import importlib.util
import json
from pathlib import Path

from PIL import Image, ImageDraw


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parent
    spec = importlib.util.spec_from_file_location('v2_export', root / 'blender-v2/run.py')
    exporter = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(exporter)
    angles = (0, 45, 90, 135, 180)
    metrics = []
    contact = Image.new('RGB', (5 * 96 + 130, 3 * (5 * 96 + 28)), '#253039')
    draw = ImageDraw.Draw(contact)
    animation_frames = [Image.new('RGB', (4 * 150, 5 * 112 + 28), '#253039') for _ in range(5)]
    reference = Image.open(root / 'reference-production-80.png').convert('RGBA')
    sheets = [('Hand drawn 16', reference, 16)]
    for index, size in enumerate((8, 12, 16)):
        sheet = Image.new('RGBA', (size * 5, size * 5))
        for row, angle in enumerate(angles):
            cells = []
            for col in range(6):
                with Image.open(args.input / str(size) / f'view{angle:03d}_frame{col:02d}.png') as image:
                    assert image.size == (size, size)
                    cells.append(exporter.indexed(image, exporter.BLUE).convert('RGBA'))
            assert cells[0].tobytes() == cells[5].tobytes(), (size, angle, 'loop')
            for col, cell in enumerate(cells[:5]):
                sheet.paste(cell, (col * size, row * size))
                enlarged = cell.resize((96, 96), Image.Resampling.NEAREST)
                contact.paste(enlarged, (130 + col * 96, index * 508 + 28 + row * 96), enlarged)
                alpha = cell.getchannel('A')
                boundary_clear = all(not alpha.getpixel((x, y)) for x in range(size) for y in range(size)
                                     if x in (0, size - 1) or y in (0, size - 1))
                metrics.append({'size': size, 'angle': angle, 'frame': col,
                                'opaque_pixels': sum(a > 0 for a in alpha.getdata()),
                                'bbox': alpha.getbbox(), 'boundary_clear': boundary_clear})
            draw.text((8, index * 508 + 28 + row * 96), f'{size}px / {angle} deg', fill='white')
        draw.text((8, index * 508 + 4), f'Native {size}px - five shared phases', fill='white')
        sheet.save(args.output / f'atlas-{size}-rgba.png')
        sheets.append((f'Blender {size}', sheet, size))
    for frame, canvas in enumerate(animation_frames):
        labels = ImageDraw.Draw(canvas)
        for column, (name, sheet, size) in enumerate(sheets):
            labels.text((column * 150 + 6, 6), name, fill='white')
            for row in range(5):
                cell = sheet.crop((frame * size, row * size, (frame + 1) * size, (row + 1) * size))
                enlarged = cell.resize((96, 96), Image.Resampling.NEAREST)
                canvas.paste(enlarged, (column * 150 + 25, 28 + row * 112), enlarged)
    contact.save(args.output / 'native-resolution-contact.png')
    animation_frames[0].save(args.output / 'resolution-comparison.gif', save_all=True,
                             append_images=animation_frames[1:], duration=200, loop=0)
    (args.output / 'metrics.json').write_text(json.dumps(metrics, indent=2) + '\n')
    print('Rendered loop endpoints match at all 3 sizes and all 5 directions.')


if __name__ == '__main__':
    main()
