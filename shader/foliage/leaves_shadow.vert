#version 450

#extension GL_GOOGLE_include_directive : require

#include "../include/core/packer.glsl"

layout(push_constant) uniform PC {
    float time;
    uvec3 chunk_world_offset;
    vec3 bottom_color;
    vec3 tip_color;
}
pc;

// these are vertex-rate attributes
layout(location = 0) in uvec2 in_packed_data;

// these are instance-rate attributes (reusing grass instance buffer)
layout(location = 1) in uint in_instance_packed_local_pos;
layout(location = 2) in uint in_instance_ty_seed;
layout(location = 3) in uint in_instance_growth_start_tick;

layout(set = 0, binding = 0) uniform U_GuiInput {
    float debug_float;
    uint debug_bool;
    uint debug_uint;
    vec3 flora_instance_hsv_offset_max;
    vec3 flora_voxel_hsv_offset_max;
    vec3 grass_bottom_dark;
    vec3 grass_bottom_light;
    vec3 grass_tip_dark;
    vec3 grass_tip_light;
    vec3 ocean_deep_color;
    vec3 ocean_shallow_color;
    float ocean_normal_amplitude;
    float ocean_noise_frequency;
    float ocean_time_multiplier;
    float ocean_sea_level_shift;
    float lens_flare_intensity;
    float lens_flare_sun_pixel_scale;
}
gui_input;

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

#include "../include/instance.glsl"
#include "../include/wind_volume.glsl"
#include "./billboard.glsl"
#include "./palette.glsl"
#include "./unpacker.glsl"

const float scaling_factor = 1.0 / 256.0;

void main() {
    ivec3 vox_local_pos;
    uvec3 vert_offset_in_vox;
    ivec3 gradient_origin;
    uint max_length;
    unpack_vertex_data(vox_local_pos, vert_offset_in_vox, gradient_origin, max_length,
                       in_packed_data);

    float wind_gradient = compute_gradient(vox_local_pos, gradient_origin, max_length);

    uvec3 instance_pos_voxels = get_instance_world_pos(in_instance_packed_local_pos, pc.chunk_world_offset);
    vec3 instance_pos = instance_pos_voxels * scaling_factor;

    uint instance_seed = decode_instance_seed(in_instance_ty_seed);
    vec3 wind_sample_pos = instance_pos + vec3(vox_local_pos) * scaling_factor;
    uint wind_seed = get_wind_volume_voxel_seed(instance_seed, vox_local_pos);
    vec3 wind_vec    = sample_wind_volume(wind_sample_pos, wind_seed);
    vec3 wind_offset = wind_vec * wind_gradient * wind_gradient;
    vec3 anchor_pos  = (vox_local_pos + wind_offset) * scaling_factor + instance_pos;
    vec3 voxel_pos   = anchor_pos + vec3(0.5) * scaling_factor;
    vec3 vert_pos    = get_vert_pos_with_billboard(shadow_camera_info.view_mat, voxel_pos,
                                                   vert_offset_in_vox, scaling_factor);

    gl_Position = shadow_camera_info.view_proj_mat * vec4(vert_pos, 1.0);

    uint palette_seed = combine_color_seed(instance_seed);
    gl_Position.z += float(in_instance_growth_start_tick & 1u) * 0.0;
    gl_Position.z += float(palette_seed & 1u) * 1e-8;
}
