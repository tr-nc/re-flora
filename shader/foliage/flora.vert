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

layout(set = 0, binding = 6) uniform U_FloraGrowthInfo {
    uint flora_tick;
    uint sprout_delay_ticks;
    uint full_growth_ticks;
}
flora_growth_info;

layout(set = 0, binding = 7) uniform U_WindVolumeInfo { vec3 world_chunk_extent; }
wind_volume_info;

layout(set = 0, binding = 8) uniform sampler3D wind_volume_tex;

#include "../include/core/color.glsl"
#include "../include/core/hash.glsl"
#include "../include/depth_offset.glsl"
#include "../include/flora_registry.glsl"
#include "../include/instance.glsl"
#include "../include/sunlight.glsl"
#define ENABLE_TEMPORAL_VSM
#include "../include/vsm.glsl"
#include "../include/leaf_shadow.glsl"
#include "../include/wind_volume.glsl"
#include "./color_variation.glsl"
#include "./grass_band_color.glsl"
#include "./palette.glsl"
#include "./unpacker.glsl"
#include "./flora_common.glsl"

layout(set = 1, binding = 0) readonly buffer B_ManualFloraInstances { Instance data[]; }
manual_flora_instances;

void main() {
    ivec3 vox_local_pos;
    uvec3 vert_offset_in_vox;
    ivec3 gradient_origin;
    uint max_length;
    unpack_vertex_data(vox_local_pos, vert_offset_in_vox, gradient_origin, max_length,
                       in_packed_data);

    bool is_grass;
    float color_gradient;
    vec3 voxel_pos;
    vec3 anchor_pos;
    float shadow_weight;
    bool should_trim_voxel;
    uint in_instance_packed_local_pos =
        manual_flora_instances.data[gl_InstanceIndex].packed_local_pos;
    uvec3 instance_pos = get_instance_world_pos(in_instance_packed_local_pos, pc.chunk_world_offset);
    uint instance_seed = get_instance_seed(instance_pos);
    uint instance_growth_progress = unpack_instance_growth_progress(in_instance_packed_local_pos);
    prepare_flora_vertex(vox_local_pos, gradient_origin, max_length, instance_pos,
                          pc.instance_ty, instance_seed, instance_growth_progress, is_grass,
                          color_gradient, voxel_pos, anchor_pos, shadow_weight, should_trim_voxel);
    vec3 vert_pos    = anchor_pos + vec3(vert_offset_in_vox) * scaling_factor;

    if (should_trim_voxel) {
        gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
        vert_color  = vec3(0.0);
        return;
    }

    // Apply depth offset to prevent z-fighting between instances
    gl_Position =
        apply_depth_offset(vert_pos, instance_pos, camera_info.view_mat, camera_info.proj_mat);

    vec3 base_color_linear =
        sample_flora_base_color(is_grass, pc.instance_ty, instance_seed, vox_local_pos,
                                instance_pos, color_gradient);

    float sun_luminance = sun_luminance_from_dir(sun_info.sun_dir, sun_info.sun_luminance);
    vec3 sun_light      = sun_info.sun_color * sun_luminance;
    vert_color = base_color_linear * (sun_light * shadow_weight + shading_info.ambient_light);
}
