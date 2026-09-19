"""Export the approved v5 rest mesh and its 25Hz articulated keyframes.

Run with Blender --background --python this-file -- --output <path>.
Reads the saved source without modifying it; no images or view atlas are baked.
Coordinates match the browser GLB (right-handed Y-up, forward -Z).
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import bpy

parser = argparse.ArgumentParser()
parser.add_argument('--output', required=True)
args = parser.parse_args(sys.argv[sys.argv.index('--')+1:])
source = Path(__file__).resolve().parent / 'blender-v5/butterfly-prototype.blend'
source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
bpy.ops.wm.open_mainfile(filepath=str(source))
scene = bpy.context.scene
meshes = sorted((o for o in scene.objects if o.type == 'MESH'), key=lambda o:o.name)
assert len(meshes) == 2 and all('wing' in o.name.lower() for o in meshes)
triangles = []
for obj in meshes:
    side = 1 if obj.name.startswith('R') else -1
    obj.data.calc_loop_triangles()
    for triangle in obj.data.loop_triangles:
        positions = []
        for index in triangle.vertices:
            x,y,z = obj.data.vertices[index].co
            positions.append([round(x,9), round(z,9), round(-y,9)])
        triangles.append({'side':side, 'positions':positions})
keys = []
for frame in range(1,27):
    scene.frame_set(frame)
    flight = bpy.data.objects['Flight pose']
    wing = bpy.data.objects['Wing hinge R']
    keys.append([-wing.rotation_euler.y, flight.rotation_euler.x, flight.location.z])
assert all(abs(a-b)<1e-6 for a,b in zip(keys[0],keys[-1]))
result = {'source_sha256':source_hash, 'source_fps':25, 'keys':keys, 'triangles':triangles}
out = Path(args.output)
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(result,separators=(',',':'))+'\n')
assert hashlib.sha256(source.read_bytes()).hexdigest() == source_hash
print(f'Runtime mesh: {len(triangles)} triangles, {len(keys)} keys, no body; {out}')
