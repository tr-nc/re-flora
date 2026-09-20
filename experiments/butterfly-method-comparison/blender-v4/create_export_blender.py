"""REVIEW-ONLY v4: wing-only geometry for freely orbiting real-time previews.

No body meshes or abdomen nodes. Fixed-view renders are regression samples only.
"""
import argparse
import json
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector

parser = argparse.ArgumentParser()
parser.add_argument("--mode", choices=("create", "export-glb", "render", "preview"), required=True)
parser.add_argument("--output", required=True)
ARGS = parser.parse_args(sys.argv[sys.argv.index("--") + 1:])
OUT = Path(ARGS.output).resolve()
OUT.mkdir(parents=True, exist_ok=True)
ANGLES = [0, 45, 90, 135, 180]
SAMPLE_FRAMES = [1, 6, 11, 16, 21]
SCALE = 3.05
ELEVATION = 60.0
POSES = {
    "wing_degrees": [5, 32, 60, 38, -18],
    "body_z": [-.04, .015, .07, .025, -.06],
    "body_pitch_degrees": [-4, 2, 8, 3, -7],
}
COLORS = [(24, 32, 48), (63, 93, 124), (80, 190, 220), (205, 246, 248)]


def periodic(values, t):
    phase = (t % 1.0) * 5
    index, f = int(phase), phase % 1
    a, b, c, d = (values[(index + offset) % 5] for offset in (-1, 0, 1, 2))
    return .5 * ((2*b) + (-a+c)*f + (2*a-5*b+4*c-d)*f*f + (-a+3*b-3*c+d)*f*f*f)


def linear(channel):
    v = channel / 255
    return v / 12.92 if v < .04045 else ((v+.055)/1.055)**2.4


def material(name, rgb):
    mat = bpy.data.materials.new(name)
    mat.diffuse_color = (*map(linear, rgb), 1)
    mat.use_nodes = True
    node = mat.node_tree.nodes.get("Principled BSDF")
    node.inputs["Base Color"].default_value = (0, 0, 0, 1)
    node.inputs["Roughness"].default_value = 1
    node.inputs["Emission Color"].default_value = mat.diffuse_color
    node.inputs["Emission Strength"].default_value = 1
    return mat


def empty(name, parent=None, location=(0, 0, 0)):
    obj = bpy.data.objects.new(name, None)
    bpy.context.collection.objects.link(obj)
    obj.parent = parent
    obj.location = location
    obj.empty_display_size = .15
    obj.empty_display_type = "PLAIN_AXES"
    return obj


def mesh(name, vertices, faces, material_indices, materials, parent, part):
    data = bpy.data.meshes.new(name)
    data.from_pydata(vertices, [], faces)
    data.update()
    obj = bpy.data.objects.new(name, data)
    bpy.context.collection.objects.link(obj)
    obj.parent = parent
    obj["part"] = part
    for mat in materials:
        data.materials.append(mat)
    for polygon, index in zip(data.polygons, material_indices):
        polygon.material_index = index
    return obj


def wing(name, outline, sign, hinge, materials):
    # A single closed low-poly wing with a broad inner color field. No tiny
    # spots, stripes, image textures, stacked markings or per-view geometry.
    cx = sum(p[0] for p in outline) / len(outline)
    cy = sum(p[1] for p in outline) / len(outline)
    inner = [(cx+(x-cx)*.86, cy+(y-cy)*.88) for x, y in outline]
    n = len(outline)
    # Cupping preserves some projected area near edge-on phases, without
    # view-specific billboarding. The same geometry is used for every camera.
    def cup(x):
        return .22 * max(0, 1 - ((x - .55) / .55) ** 2)
    verts = [(sign*x, y, z+cup(x)) for points, z in ((outline,.05),(inner,.05),(outline,-.05),(inner,-.05)) for x,y in points]
    faces, indices = [], []
    for i in range(n):
        j = (i+1)%n
        faces += [(i,j,n+j,n+i),(2*n+i,3*n+i,3*n+j,2*n+j),(i,2*n+i,2*n+j,j)]
        indices += [2,2,2]
    # One shared contour for the fore/hind lobes avoids a dark seam that read
    # as detached flower petals at 16px. The two large top color fields share
    # edges; there is no black internal outline or decorative micro-pattern.
    verts.append((sign*cx,cy,.05+cup(cx)))
    for i in range(n):
        j=(i+1)%n
        faces.append((4*n,n+i,n+j))
        indices.append(3 if (inner[i][1]+inner[j][1])/2 > -.10 else 2)
    faces.append(tuple(reversed(range(3*n,4*n))))
    indices.append(2)
    return mesh(name,verts,faces,indices,materials,hinge,"wing")


def camera_pose(camera, azimuth, elevation=ELEVATION):
    az, el = math.radians(azimuth), math.radians(elevation)
    camera.location = Vector((-math.sin(az)*math.cos(el),math.cos(az)*math.cos(el),math.sin(el)))*7
    camera.rotation_euler = (-camera.location).to_track_quat("-Z","Y").to_euler()


def setup_render(scene, size, pixel=False):
    scene.render.resolution_x = scene.render.resolution_y = size
    scene.cycles.samples = 1 if pixel else 8
    scene.cycles.pixel_filter_type = "BOX" if pixel else "BLACKMAN_HARRIS"
    scene.cycles.filter_width = .01 if pixel else 1.5


def create():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.use_denoising = False
    scene.cycles.seed = 0
    scene.cycles.use_animated_seed = False
    scene.render.threads_mode = "FIXED"
    scene.render.threads = 4
    scene.render.resolution_percentage = 100
    scene.render.film_transparent = True
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.color_depth = "8"
    scene.render.fps = 25
    scene.frame_start, scene.frame_end = 1, 25
    scene.view_settings.view_transform = "Standard"
    scene.view_settings.look = "None"
    scene.view_settings.exposure = 0
    scene.view_settings.gamma = 1
    scene.world.color = (0,0,0)
    setup_render(scene,128)
    mats = [material(name,rgb) for name,rgb in zip(("Boundary","Body facet","Main wing","Light wing"),COLORS)]
    origin = empty("Fixed world reference")
    flight = empty("Flight pose",origin)
    # Deliberately wing-only: no hidden body, head, thorax or abdomen geometry.
    # No subpixel antennae or full black wing border at 12px. Broader roots
    # connect the two lobes; one waist is enough to suggest fore/hind wings.
    wing_outline = [(.07,.32),(.32,.77),(.78,.97),(1.10,.75),(1.12,.32),(.84,-.08),(.98,-.43),(.72,-.84),(.35,-.82),(.08,-.40)]
    hinges=[]
    for side,label in ((1,"R"),(-1,"L")):
        hinge=empty("Wing hinge "+label,flight)
        hinges.append((side,hinge))
        wing(label+" continuous fore-hind wing",wing_outline,side,hinge,mats)
    for frame in range(1,27):
        t=(frame-1)/25
        flight.location.z=periodic(POSES["body_z"],t)
        flight.rotation_euler.x=math.radians(periodic(POSES["body_pitch_degrees"],t))
        flight.keyframe_insert(data_path="location",index=2,frame=frame)
        flight.keyframe_insert(data_path="rotation_euler",index=0,frame=frame)
        for side,hinge in hinges:
            hinge.rotation_euler.y=-side*math.radians(periodic(POSES["wing_degrees"],t))
            hinge.keyframe_insert(data_path="rotation_euler",index=1,frame=frame)
    for obj in (flight,*[h for _,h in hinges]):
        obj.animation_data.action.name="Coordinated wingbeat: "+obj.name
        for curve in obj.animation_data.action.fcurves:
            for key in curve.keyframe_points:
                key.interpolation="LINEAR"
            curve.modifiers.new("CYCLES")
    for angle in ANGLES:
        data=bpy.data.cameras.new("Orthographic "+str(angle))
        data.type="ORTHO"
        data.ortho_scale=SCALE
        camera=bpy.data.objects.new(f"View {angle:03d}",data)
        bpy.context.collection.objects.link(camera)
        camera_pose(camera,angle)
    scene.camera=bpy.data.objects["View 045"]
    scene.frame_set(11)
    scene["PROTOTYPE"]="Butterfly v4: two wing meshes only. Free camera runtime source; fixed render samples are diagnostics."
    scene["PIXEL_RENDER"]="12/16px native Cycles single sample, BOX filter width .01, fixed palette + binary alpha."
    scene["POSES"] = json.dumps(POSES)
    bpy.ops.wm.save_as_mainfile(filepath=str(OUT/"butterfly-prototype.blend"))
    meshes=[o for o in scene.objects if o.type=="MESH"]
    manifest={
        "prototype":True,"version":4,"blender_version":bpy.app.version_string,
        "blend":"butterfly-prototype.blend","glb":"butterfly-prototype.glb",
        "geometry":{"mesh_objects":len(meshes),"faces":sum(len(o.data.polygons) for o in meshes),"wing_design":"Exactly two cupped wing meshes. No body/head/thorax/abdomen meshes; no hidden body parts. Camera-independent geometry."},
        "camera_elevation_degrees":ELEVATION,"orthographic_scale":SCALE,"camera_target_world":[0,0,0],
        "fixed_world_reference":[0,0,0],"intentional_body_motion":True,"per_frame_recentering":False,
        "view_azimuth_degrees":ANGLES,"sample_frames":SAMPLE_FRAMES,"sample_seconds":[0,.2,.4,.6,.8],
        "source_fps":25,"loop_seconds":1,"source_frame_range":[1,25],"loop_endpoint_frame":26,
        "key_pose_table":POSES,"curve":"Periodic Catmull-Rom authored poses sampled at 25fps, linear keys; identical t=1 endpoint.",
        "required_animated_nodes":["Flight pose","Wing hinge R","Wing hinge L"],
        "raw_frame_px":128,"primary_pixel_frame_px":12,
        "primary_pixel_method":"Native 12px render: 1 sample, BOX filter width .01, fixed seed. 16px comparison; no image resizing.",
        "palette_srgb":COLORS,"postprocess":"Fixed global palette quantization, binary alpha threshold128, no dithering, no centering.",
        "art_basis":"Hand-authored sprite observation + 3D-to-pixel workflow practice; motion is stylized design, not recovered anatomy or biomechanics."
    }
    (OUT/"source-manifest.json").write_text(json.dumps(manifest,indent=2)+"\n")


def export_glb():
    scene=bpy.context.scene
    bpy.ops.object.select_all(action="DESELECT")
    for obj in scene.objects:
        obj.select_set(obj.type in ("MESH","EMPTY"))
    scene.frame_end=26
    bpy.ops.export_scene.gltf(filepath=str(OUT/"butterfly-prototype.glb"),export_format="GLB",use_selection=True,
        export_animations=True,export_frame_range=True,export_force_sampling=True,
        export_animation_mode="SCENE",export_anim_scene_split_object=False,export_anim_slide_to_zero=True,
        export_cameras=False,export_lights=False)


def render_image(scene,folder,name,size,pixel=False):
    folder.mkdir(parents=True,exist_ok=True)
    setup_render(scene,size,pixel)
    scene.render.filepath=str(folder/name)
    bpy.ops.render.render(write_still=True)


def render():
    scene=bpy.context.scene
    for angle in ANGLES:
        scene.camera=bpy.data.objects[f"View {angle:03d}"]
        for col,frame in enumerate([*SAMPLE_FRAMES,26]):
            scene.frame_set(frame)
            name=f"view{angle:03d}_frame{col:02d}.png"
            for size in (12,16,128):
                render_image(scene,OUT/f"native{size}",name,size,size<128)


def preview():
    scene=bpy.context.scene
    scene.camera=bpy.data.objects["View 045"]
    for index in range(60):
        camera_pose(scene.camera,index*360/60,35)
        timeline=1+(index*100/60)%25
        scene.frame_set(int(timeline),subframe=timeline%1)
        render_image(scene,OUT,f"turntable_{index:03d}.png",256)


{"create":create,"export-glb":export_glb,"render":render,"preview":preview}[ARGS.mode]()
