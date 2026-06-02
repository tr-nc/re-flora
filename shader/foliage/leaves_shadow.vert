#version 450

#extension GL_GOOGLE_include_directive : require

#include "../include/core/packer.glsl"

layout(push_constant) uniform PC {
    float time;
    uint instance_ty;
    uvec3 chunk_world_offset;
    vec3 bottom_color;
    vec3 tip_color;
}
pc;

// these are vertex-rate attributes
layout(location = 0) in uvec2 in_packed_data;

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

layout(set = 0, binding = 3) uniform U_ShadowCameraInfo {
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

layout(set = 0, binding = 7) uniform U_WindVolumeInfo { vec3 world_chunk_extent; }
wind_volume_info;

layout(set = 0, binding = 8) uniform sampler3D wind_volume_tex;

#include "../include/core/hash.glsl"
#include "../include/instance.glsl"
#include "../include/tree_leaf_instance.glsl"
#include "../include/wind_volume.glsl"
#include "./billboard.glsl"
#include "./palette.glsl"
#include "./unpacker.glsl"
#include "./flora_wind_motion.glsl"

layout(set = 1, binding = 0) readonly buffer B_TreeLeafInstances { TreeLeafInstance data[]; }
manual_tree_leaf_instances;

layout(location = 0) out float vert_leaf_shadow_opacity;

const float scaling_factor = 1.0 / 256.0;

void main() {
    ivec3 vox_local_pos;
    uvec3 vert_offset_in_vox;
    ivec3 gradient_origin;
    uint max_length;
    unpack_vertex_data(vox_local_pos, vert_offset_in_vox, gradient_origin, max_length,
                       in_packed_data);

    float wind_gradient = compute_gradient(vox_local_pos, gradient_origin, max_length);

    uint in_instance_packed_local_pos =
        manual_tree_leaf_instances.data[gl_InstanceIndex].packed_local_pos;
    uvec3 instance_pos_voxels = get_tree_leaf_world_pos(in_instance_packed_local_pos, pc.chunk_world_offset);
    vec3 instance_pos = instance_pos_voxels * scaling_factor;

    uint instance_seed = get_instance_seed(instance_pos_voxels);
    bool is_apple = pc.instance_ty == FLORA_SPECIES_APPLE;
    vec3 wind_sample_pos = is_apple ? instance_pos : instance_pos + vec3(vox_local_pos) * scaling_factor;
    uint wind_seed = is_apple ? instance_seed : get_wind_volume_voxel_seed(instance_seed, vox_local_pos);
    vec3 wind_vec = sample_wind_volume(wind_sample_pos, wind_seed);
    vec3 wind_offset = wind_vec * wind_gradient * wind_gradient;
    float wind_motion_time =
        wind_volume_bucket_update_time(get_wind_volume_bucket_index(wind_seed), pc.time);
    if (is_apple) {
        wind_offset = apple_wind_swing(wind_vec, instance_seed, wind_motion_time);
    } else {
        wind_offset += leaf_wind_paddling(wind_vec, wind_gradient, instance_seed, vox_local_pos,
                                          gradient_origin, wind_motion_time);
    }
    vec3 anchor_pos = (vec3(vox_local_pos) + wind_offset) * scaling_factor + instance_pos;
    vec3 voxel_pos   = anchor_pos + vec3(0.5) * scaling_factor;
    vec3 vert_pos    = get_vert_pos_with_billboard(shadow_camera_info.view_mat, voxel_pos,
                                                   vert_offset_in_vox, scaling_factor);

    gl_Position = shadow_camera_info.view_proj_mat * vec4(vert_pos, 1.0);
    vert_leaf_shadow_opacity = clamp(gui_input.leaf_shadow_fragment_opacity, 0.0, 1.0);

    uint palette_seed = combine_color_seed(instance_seed);
    gl_Position.z += float(palette_seed & 1u) * 1e-8;
}
