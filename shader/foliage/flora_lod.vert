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

// these are instance-rate attributes
layout(location = 1) in uint in_instance_packed_local_pos;
// Unused padding keeps the instance vertex binding at an 8-byte stride.
layout(location = 2) in uint in_instance_padding;

layout(location = 0) out vec3 vert_color;

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
#include "../include/flora_registry.glsl"
#include "../include/instance.glsl"
#include "../include/sunlight.glsl"
#include "../include/vsm.glsl"
#include "../include/wind_volume.glsl"
#include "./billboard.glsl"
#include "./color_variation.glsl"
#include "./grass_band_color.glsl"
#include "./palette.glsl"
#include "./unpacker.glsl"
#include "./flora_common.glsl"

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
    uvec3 instance_pos = get_instance_world_pos(in_instance_packed_local_pos, pc.chunk_world_offset);
    uint instance_seed = get_instance_seed(instance_pos);
    uint instance_growth_progress = unpack_instance_growth_progress(in_instance_packed_local_pos);
    prepare_flora_vertex(vox_local_pos, gradient_origin, max_length, instance_pos,
                          pc.instance_ty, instance_seed, instance_growth_progress, is_grass,
                          color_gradient, voxel_pos, anchor_pos, shadow_weight, should_trim_voxel);
    vec3 vert_pos = get_vert_pos_with_billboard(camera_info.view_mat, voxel_pos, vert_offset_in_vox,
                                                scaling_factor);

    if (should_trim_voxel) {
        gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
        vert_color  = vec3(0.0);
        return;
    }

    gl_Position = camera_info.view_proj_mat * vec4(vert_pos, 1.0);

    vec3 base_color_linear =
        sample_flora_base_color(is_grass, pc.instance_ty, instance_seed, vox_local_pos,
                                instance_pos, color_gradient);

    float sun_luminance = sun_luminance_from_dir(sun_info.sun_dir, sun_info.sun_luminance);
    vec3 sun_light      = sun_info.sun_color * sun_luminance;
    vert_color = base_color_linear * (sun_light * shadow_weight + shading_info.ambient_light);
}
