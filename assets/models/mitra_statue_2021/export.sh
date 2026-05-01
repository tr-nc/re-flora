#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BLEND_FILE="${1:-}"
if [[ -z "$BLEND_FILE" ]]; then
  BLEND_FILE="$SCRIPT_DIR/mitra_statue_2021.blend"
fi

OUT_DIR="$SCRIPT_DIR/glb"
mkdir -p "$OUT_DIR"

if command -v blender >/dev/null 2>&1; then
  BLENDER=(blender)
elif command -v flatpak >/dev/null 2>&1 && flatpak info org.blender.Blender >/dev/null 2>&1; then
  BLENDER=(flatpak run org.blender.Blender)
else
  echo "Blender not found. Install blender or Flatpak org.blender.Blender." >&2
  exit 127
fi

OUT_DIR="$OUT_DIR" "${BLENDER[@]}" --background "$BLEND_FILE" --python-expr "$(cat <<'PY'
import os
import re
import bpy
from mathutils import Vector

out_dir = os.environ["OUT_DIR"]

def safe_name(name):
    name = re.sub(r"[^A-Za-z0-9_.-]+", "_", name.strip())
    return name or "object"

mesh_objects = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
if not mesh_objects:
    raise SystemExit("No mesh objects found to export")

original_active = bpy.context.view_layer.objects.active
original_selection = [obj for obj in bpy.context.selected_objects]

for obj in mesh_objects:
    original_matrix = obj.matrix_world.copy()
    world_corners = [obj.matrix_world @ Vector(corner) for corner in obj.bound_box]
    center = sum(world_corners, Vector()) / len(world_corners)

    bpy.ops.object.select_all(action="DESELECT")
    obj.matrix_world.translation -= center
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj

    filepath = os.path.join(out_dir, safe_name(obj.name) + ".glb")
    bpy.ops.export_scene.gltf(
        filepath=filepath,
        export_format="GLB",
        export_yup=True,
        use_selection=True,
    )

    obj.matrix_world = original_matrix
    print(f"Exported {obj.name} -> {filepath}")

bpy.ops.object.select_all(action="DESELECT")
for obj in original_selection:
    obj.select_set(True)
bpy.context.view_layer.objects.active = original_active
PY
)"
