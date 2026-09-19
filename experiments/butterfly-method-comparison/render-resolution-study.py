"""Diagnostic only: render the saved v2 model at 8/12/16px without changing it.

Run with Blender --background --factory-startup --python THIS_FILE -- --output DIR.
Keeps the saved cameras, poses, framing and materials; does not save the .blend.
"""
import argparse
from pathlib import Path
import sys

import bpy


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args(sys.argv[sys.argv.index('--') + 1:])
    source = Path(__file__).resolve().parent / 'blender-v2/butterfly-prototype.blend'
    output = args.output.resolve()
    if output == source.parent or source.parent in output.parents:
        parser.error('Use a separate study directory, not the archived v2 directory')
    bpy.ops.wm.open_mainfile(filepath=str(source))
    scene = bpy.context.scene
    scene.cycles.samples = 1
    scene.cycles.pixel_filter_type = 'BOX'
    scene.cycles.filter_width = .01
    for size in (8, 12, 16):
        folder = output / str(size)
        folder.mkdir(parents=True, exist_ok=True)
        scene.render.resolution_x = scene.render.resolution_y = size
        for angle in (0, 45, 90, 135, 180):
            scene.camera = bpy.data.objects[f'View {angle:03d}']
            for col, frame in enumerate((1, 6, 11, 16, 21, 26)):
                scene.frame_set(frame)
                scene.render.filepath = str(folder / f'view{angle:03d}_frame{col:02d}.png')
                bpy.ops.render.render(write_still=True)


if __name__ == '__main__':
    main()
