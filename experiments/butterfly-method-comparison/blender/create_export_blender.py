"""THROWAWAY PROTOTYPE: one real model and animation, five synchronized cameras.

Run through run.py. All renders derive from the saved .blend, never from 2D art.
"""
import argparse
import json
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector


def arguments():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True)
    parser.add_argument("--mode", choices=("create", "render", "preview", "export-glb"), required=True)
    return parser.parse_args(sys.argv[sys.argv.index("--") + 1 :])


ARGS = arguments()
OUT = Path(ARGS.output).resolve()
OUT.mkdir(parents=True, exist_ok=True)
ANGLES = [0, 45, 90, 135, 180]
FRAMES = [1, 6, 11, 16, 21]
SCALE = 2.6
ELEVATION = 15.0


def linear(channel):
    value = channel / 255.0
    return value / 12.92 if value < 0.04045 else ((value + 0.055) / 1.055) ** 2.4


def material(name, color):
    result = bpy.data.materials.new(name)
    result.diffuse_color = (*[linear(c) for c in color], 1)
    result.use_nodes = True
    node = result.node_tree.nodes.get("Principled BSDF")
    node.inputs["Base Color"].default_value = (0, 0, 0, 1)
    node.inputs["Roughness"].default_value = 1
    node.inputs["Emission Color"].default_value = result.diffuse_color
    node.inputs["Emission Strength"].default_value = 1
    return result


def polygon(name, points, mat, parent=None):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(points, [], [tuple(range(len(points)))])
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    obj.data.materials.append(mat)
    obj.parent = parent
    return obj


def wing(name, xy, sign, parent, mats):
    # Outer silhouette, one inset fill and two geometric markings. No texture,
    # no independent per-view geometry and no rescaled/per-frame anchoring.
    polygon(name + "_border", [(sign * x, y, 0) for x, y in xy], mats[0], parent)
    cx = sum(x for x, _ in xy) / len(xy)
    cy = sum(y for _, y in xy) / len(xy)
    inner = [(cx + (x - cx) * 0.82, cy + (y - cy) * 0.82) for x, y in xy]
    for z in (-0.004, 0.004):
        polygon(name + "_cyan", [(sign * x, y, z) for x, y in inner], mats[2], parent)
        stripe = [(cx + (x - cx) * 0.60, cy + (y - cy) * 0.60 - 0.035) for x, y in xy]
        polygon(name + "_blue", [(sign * x, y, z * 2) for x, y in stripe], mats[1], parent)
        spot = [(cx + (x - cx) * 0.21 + 0.07, cy + (y - cy) * 0.24 + 0.10) for x, y in xy]
        polygon(name + "_highlight", [(sign * x, y, z * 3) for x, y in spot], mats[3], parent)


def ellipsoid(name, location, scale, mat):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=8, ring_count=4, location=location)
    obj = bpy.context.object
    obj.name = name
    obj.scale = scale
    obj.data.materials.append(mat)
    return obj


def camera_pose(camera, azimuth, elevation=ELEVATION):
    az, el = math.radians(azimuth), math.radians(elevation)
    camera.location = Vector((-math.sin(az) * math.cos(el), math.cos(az) * math.cos(el), math.sin(el))) * 7
    camera.rotation_euler = (-camera.location).to_track_quat("-Z", "Y").to_euler()


def configure_scene():
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 8
    scene.cycles.use_denoising = False
    scene.cycles.seed = 0
    scene.cycles.use_animated_seed = False
    scene.render.threads_mode = "FIXED"
    scene.render.threads = 4
    scene.render.resolution_x = scene.render.resolution_y = 128
    scene.render.resolution_percentage = 100
    scene.render.film_transparent = True
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.color_depth = "8"
    scene.render.image_settings.compression = 15
    scene.render.fps = 25
    scene.frame_start, scene.frame_end = 1, 25
    scene.view_settings.view_transform = "Standard"
    scene.view_settings.look = "None"
    scene.view_settings.exposure = 0
    scene.view_settings.gamma = 1
    scene.world.color = (0, 0, 0)
    return scene


def create():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    scene = configure_scene()
    mats = [material(name, rgb) for name, rgb in [
        ("01 Body and wing border", (20, 27, 40)),
        ("02 Blue wing inset", (48, 108, 165)),
        ("03 Cyan wing margin", (74, 194, 228)),
        ("04 Pale cyan highlight", (190, 246, 250)),
    ]]
    fore = [(0.035, 0.06), (0.34, 0.59), (0.78, 0.75), (0.96, 0.55), (0.79, 0.10), (0.35, -0.17)]
    hind = [(0.03, -0.035), (0.39, -0.10), (0.71, -0.27), (0.62, -0.57), (0.32, -0.68), (0.065, -0.36)]
    for sign, side in ((1, "R"), (-1, "L")):
        hinge = bpy.data.objects.new("Wing hinge " + side, None)
        bpy.context.collection.objects.link(hinge)
        hinge.empty_display_type = "PLAIN_AXES"
        hinge.empty_display_size = 0.16
        hinge.rotation_mode = "XYZ"
        wing(side + " forewing", fore, sign, hinge, mats)
        wing(side + " hindwing", hind, sign, hinge, mats)
        # One 1-second physical animation shared across all five cameras.
        for frame in range(1, 27):
            angle = 0.0 if frame in (1, 26) else math.radians(60) * math.sin(2 * math.pi * (frame - 1) / 25)
            hinge.rotation_euler.y = -sign * angle
            hinge.keyframe_insert(data_path="rotation_euler", index=1, frame=frame)
        action = hinge.animation_data.action
        action.name = "Shared 1 second wingbeat " + side
        for fcurve in action.fcurves:
            for key in fcurve.keyframe_points:
                key.interpolation = "BEZIER"
                key.handle_left_type = key.handle_right_type = "AUTO_CLAMPED"
            fcurve.modifiers.new("CYCLES")
    ellipsoid("Abdomen", (0, -0.16, 0), (0.065, 0.32, 0.065), mats[0])
    ellipsoid("Thorax", (0, 0.10, 0.012), (0.092, 0.17, 0.083), mats[1])
    ellipsoid("Head", (0, 0.285, 0.025), (0.085, 0.085, 0.078), mats[0])
    for sign in (-1, 1):
        polygon("Antenna", [(sign * 0.025, .30, .065), (sign * .11, .50, .12), (sign * .09, .50, .12), (sign * .012, .30, .065)], mats[0])
    for azimuth in ANGLES:
        data = bpy.data.cameras.new("Orthographic " + str(azimuth))
        data.type = "ORTHO"
        data.ortho_scale = SCALE
        camera = bpy.data.objects.new("View " + str(azimuth).zfill(3), data)
        bpy.context.collection.objects.link(camera)
        camera_pose(camera, azimuth)
    scene.camera = bpy.data.objects["View 045"]
    scene.frame_set(6)
    scene["PROTOTYPE"] = "Review only. Not a production replacement. Created solely from geometry, no image textures."
    scene["EXPORT_CONTRACT"] = "Five rows: 0,45,90,135,180 deg azimuth. Five columns t=0,.2,.4,.6,.8s. Body origin maps to frame center."
    scene["WING_ANGLE"] = "60 degrees * sin(2*pi*t), 1s period; 25fps editable source action."
    bpy.ops.wm.save_as_mainfile(filepath=str(OUT / "butterfly-prototype.blend"))
    mesh_objects = [o for o in scene.objects if o.type == "MESH"]
    metadata = {
        "prototype": True, "blender_version": bpy.app.version_string,
        "blend": "butterfly-prototype.blend", "glb": "butterfly-prototype.glb",
        "geometry": {"mesh_objects": len(mesh_objects), "faces": sum(len(o.data.polygons) for o in mesh_objects)},
        "raw_frame_px": 128, "view_azimuth_degrees": ANGLES,
        "camera_elevation_degrees": ELEVATION, "orthographic_scale": SCALE,
        "camera_target_world": [0, 0, 0], "body_anchor_normalized": [.5, .5],
        "source_fps": 25, "source_frame_range": [1, 25], "loop_endpoint_frame": 26,
        "sample_frames": FRAMES, "sample_seconds": [0, .2, .4, .6, .8],
        "sample_wing_angle_degrees": [60 * math.sin(2 * math.pi * (f - 1) / 25) for f in FRAMES],
        "loop_seconds": 1, "sheet_layout": "row = camera, column = time; no gutters",
        "source_transparency": "RGBA, antialiased coverage; not indexed or binary alpha",
        "renderer": "Cycles CPU, 8 samples, seed 0, animated seed off, Standard view transform",
        "pose_consistency": "One saved model and shared keyed hinges; only camera varies between rows.",
    }
    (OUT / "source-manifest.json").write_text(json.dumps(metadata, indent=2) + "\n")
    export_glb()


def export_glb():
    # GLB is a real animated 3D export of this same scene, not an independent mockup.
    scene = bpy.context.scene
    bpy.ops.object.select_all(action="DESELECT")
    for obj in scene.objects:
        if obj.type in ("MESH", "EMPTY"):
            obj.select_set(True)
    scene.frame_end = 26
    bpy.ops.export_scene.gltf(filepath=str(OUT / "butterfly-prototype.glb"), export_format="GLB", use_selection=True,
        export_animations=True, export_frame_range=True, export_force_sampling=True,
        export_animation_mode="SCENE", export_anim_scene_split_object=False,
        export_anim_slide_to_zero=True,
        export_cameras=False, export_lights=False)


def render():
    scene = bpy.context.scene
    OUT.mkdir(parents=True, exist_ok=True)
    for azimuth in ANGLES:
        scene.camera = bpy.data.objects["View " + str(azimuth).zfill(3)]
        for index, frame in enumerate(FRAMES):
            scene.frame_set(frame)
            scene.render.filepath = str(OUT / f"view{azimuth:03d}_frame{index:02d}.png")
            bpy.ops.render.render(write_still=True)
    # Verify the true t=1 endpoint against t=0, distinct from last sampled t=.8.
    scene.camera = bpy.data.objects["View 045"]
    scene.frame_set(26)
    scene.render.filepath = str(OUT / "loop_endpoint_view045.png")
    bpy.ops.render.render(write_still=True)


def preview():
    scene = bpy.context.scene
    scene.render.resolution_x = scene.render.resolution_y = 256
    scene.camera = bpy.data.objects["View 045"]
    # Four wingbeat cycles during a complete 360-degree orbit. Raw Blender renders.
    for index in range(60):
        camera_pose(scene.camera, index * 360 / 60, 22)
        timeline = 1 + (index * 100 / 60) % 25
        scene.frame_set(int(timeline), subframe=timeline % 1)
        scene.render.filepath = str(OUT / f"turntable_{index:03d}.png")
        bpy.ops.render.render(write_still=True)


{"create": create, "render": render, "preview": preview, "export-glb": export_glb}[ARGS.mode]()
