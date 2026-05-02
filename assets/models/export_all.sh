#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

if command -v blender >/dev/null 2>&1; then
  BLENDER=(blender)
elif command -v flatpak >/dev/null 2>&1 && flatpak info org.blender.Blender >/dev/null 2>&1; then
  BLENDER=(flatpak run org.blender.Blender)
else
  echo "Blender not found. Install blender or Flatpak org.blender.Blender." >&2
  exit 127
fi

if [[ "$#" -gt 0 ]]; then
  BLEND_FILES=("$@")
else
  mapfile -d '' BLEND_FILES < <(find "$SCRIPT_DIR" -type f -name '*.blend' -print0 | sort -z)
fi

if [[ "${#BLEND_FILES[@]}" -eq 0 ]]; then
  echo "No .blend files found under $SCRIPT_DIR" >&2
  exit 1
fi

for BLEND_FILE in "${BLEND_FILES[@]}"; do
  BLEND_DIR="$(cd -- "$(dirname -- "$BLEND_FILE")" && pwd)"
  OUT_DIR="$BLEND_DIR"

  echo "Exporting $BLEND_FILE"
  SCRIPT_DIR="$SCRIPT_DIR" BLEND_DIR="$BLEND_DIR" OUT_DIR="$OUT_DIR" "${BLENDER[@]}" --background "$BLEND_FILE" --python-expr "$(cat <<'PY'
import os
import re
import shutil
import bpy

script_dir = os.environ["SCRIPT_DIR"]
blend_dir = os.environ["BLEND_DIR"]
out_dir = os.environ["OUT_DIR"]

def safe_name(name):
    name = re.sub(r"[^A-Za-z0-9_.-]+", "_", name.strip())
    return name or "object"

mesh_objects = [obj for obj in bpy.context.scene.objects if obj.type == "MESH"]
if not mesh_objects:
    raise SystemExit("No mesh objects found to export")

def export_dir_for(obj):
    export_subdir = obj.get("export_subdir")
    if export_subdir and os.path.abspath(blend_dir) == os.path.abspath(script_dir):
        return os.path.join(script_dir, export_subdir)
    return out_dir

for target_dir in sorted({export_dir_for(obj) for obj in mesh_objects}):
    legacy_glb_dir = os.path.join(target_dir, "glb")
    shutil.rmtree(legacy_glb_dir, ignore_errors=True)
    os.makedirs(target_dir, exist_ok=True)
    for filename in os.listdir(target_dir):
        if filename.endswith(".glb"):
            os.remove(os.path.join(target_dir, filename))

original_active = bpy.context.view_layer.objects.active
original_selection = [obj for obj in bpy.context.selected_objects]

for obj in mesh_objects:
    original_matrix = obj.matrix_world.copy()

    for scene_obj in bpy.context.scene.objects:
        scene_obj.select_set(False)
    obj.matrix_world.translation = (0.0, 0.0, 0.0)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj

    filepath = os.path.join(export_dir_for(obj), safe_name(obj.name) + ".glb")
    bpy.ops.export_scene.gltf(
        filepath=filepath,
        export_format="GLB",
        export_yup=True,
        export_materials="NONE",
        export_texcoords=False,
        export_tangents=False,
        export_vertex_color="NONE",
        export_attributes=False,
        export_cameras=False,
        export_lights=False,
        export_animations=False,
        export_skins=False,
        export_morph=False,
        export_extras=False,
        use_selection=True,
    )

    obj.matrix_world = original_matrix
    print(f"Exported {obj.name} -> {filepath}")

for obj in bpy.context.scene.objects:
    obj.select_set(False)
for obj in original_selection:
    obj.select_set(True)
bpy.context.view_layer.objects.active = original_active
PY
)"
done
