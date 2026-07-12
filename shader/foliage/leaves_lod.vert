#version 450

#extension GL_GOOGLE_include_directive : require

#include "../include/core/packer.glsl"
#include "./flora_push_constant.glsl"

layout(location = 0) in uint in_packed_data;

layout(location = 0) out vec3 vert_color;

#include "../include/gui_input.glsl"

layout(set = 0, binding = 1) uniform U_SunInfo {
    vec3 sun_dir;
    float sun_size;
    vec3 sun_color;
    float sun_luminance;
    float sun_display_luminance;
    float sun_altitude;
    float sun_azimuth;
}
sun_info;

layout(set = 0, binding = 2) uniform U_ShadingInfo { vec3 ambient_light; }
shading_info;

layout(set = 0, binding = 3) uniform U_CameraInfo {
    vec4 pos;
    mat4 view_mat;
    mat4 view_mat_inv;
    mat4 proj_mat;
    mat4 proj_mat_inv;
    mat4 view_proj_mat;
    mat4 view_proj_mat_inv;
}
camera_info;

layout(set = 0, binding = 4) uniform U_ShadowCameraInfo {
    vec4 pos;
    mat4 view_mat;
    mat4 view_mat_inv;
    mat4 proj_mat;
    mat4 proj_mat_inv;
    mat4 view_proj_mat;
    mat4 view_proj_mat_inv;
}
shadow_camera_info;

layout(set = 0, binding = 5) uniform sampler2D shadow_map_tex_for_vsm_ping;
layout(set = 0, binding = 9) uniform sampler2D leaf_shadow_opacity_blended_tex;
layout(set = 0, binding = 10) uniform sampler2D leaf_shadow_mask_tex;
layout(set = 0, binding = 11) uniform sampler2D cloud_shadow_tex;

#include "./flora_animation_info.glsl"

layout(set = 0, binding = 7) uniform U_WindVolumeInfo { vec3 world_chunk_extent; }
wind_volume_info;

layout(set = 0, binding = 8) uniform sampler3D wind_volume_tex;

#include "../include/core/color.glsl"
#include "../include/core/hash.glsl"
#include "../include/flora_registry.glsl"
#include "../include/instance.glsl"
#include "../include/sunlight.glsl"
#include "../include/tree_leaf_instance.glsl"
#define ENABLE_TEMPORAL_VSM
#include "../include/vsm.glsl"
#include "../include/leaf_shadow.glsl"
#include "../include/cloud_shadow.glsl"
#define TERRAIN_EDIT_PREVIEW_BINDING 12
#include "../include/terrain_edit_preview.glsl"
#include "../include/wind_volume.glsl"
#include "./billboard.glsl"
#include "./color_variation.glsl"
#include "./grass_band_color.glsl"
#include "./palette.glsl"
#include "./unpacker.glsl"
#include "./flora_voxel_lookup.glsl"
#include "./flora_common.glsl"

layout(set = 1, binding = 0) readonly buffer B_TreeLeafInstances { TreeLeafInstance data[]; }
manual_tree_leaf_instances;

void main() {
    ivec3 vox_local_pos;
    uvec3 vert_offset_in_vox;
    unpack_vertex_data(vox_local_pos, vert_offset_in_vox, in_packed_data);

    bool is_grass;
    float color_gradient;
    vec3 voxel_pos;
    vec3 anchor_pos;
    float shadow_weight;
    bool should_trim_voxel;
    TreeLeafInstance tree_leaf_instance = manual_tree_leaf_instances.data[gl_InstanceIndex];
    uvec3 leaf_voxel_world_pos =
        get_tree_leaf_world_pos(tree_leaf_instance.packed_local_pos, pc.chunk_world_offset);
    bool is_tree_leaf = pc.instance_ty == FLORA_SPECIES_TREE_LEAF;
    ivec3 leaf_vox_local_pos = is_tree_leaf
                                   ? unpack_tree_leaf_voxel_local_pos(
                                         tree_leaf_instance.packed_leaf_local_pos)
                                   : vox_local_pos;
    uvec3 instance_pos = is_tree_leaf ? uvec3(ivec3(leaf_voxel_world_pos) - leaf_vox_local_pos)
                                      : leaf_voxel_world_pos;
    uint instance_seed = get_instance_seed(instance_pos);
    uint voxel_info = lookup_flora_voxel_info(pc.instance_ty, leaf_vox_local_pos);
    prepare_flora_vertex(leaf_vox_local_pos, voxel_info, instance_pos, pc.instance_ty,
                          instance_seed, TREE_LEAF_GROWTH_PROGRESS, INSTANCE_SPAWN_INACTIVE,
                          is_grass, color_gradient, voxel_pos, anchor_pos, shadow_weight,
                          should_trim_voxel);
    vec3 vert_pos = get_vert_pos_with_billboard(camera_info.view_mat, voxel_pos, vert_offset_in_vox,
                                                scaling_factor);

    if (should_trim_voxel) {
        gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
        vert_color = vec3(0.0);
        return;
    }

    gl_Position = camera_info.view_proj_mat * vec4(vert_pos, 1.0);

    vec3 base_color_linear =
        sample_flora_base_color(is_grass, pc.instance_ty, instance_seed, leaf_vox_local_pos,
                                instance_pos, color_gradient, voxel_info);

    vec3 lit_color = apply_stylized_voxel_lighting(base_color_linear, shadow_weight);
    // The edit brush is a UI affordance, not foliage material. Match the terrain tracer by
    // applying the tint after lighting so rasterized voxels highlight consistently.
    vert_color = apply_terrain_edit_preview_tint(lit_color, voxel_pos);
}
