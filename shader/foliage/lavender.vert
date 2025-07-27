#version 450

#extension GL_GOOGLE_include_directive : require

#include "../include/core/packer.glsl"

// these are vertex-rate attributes
layout(location = 0) in uint in_packed_data;

// these are instance-rate attributes
layout(location = 1) in uvec3 in_instance_pos;
layout(location = 2) in uint in_instance_ty;

layout(location = 0) out vec3 vert_color;

layout(set = 0, binding = 0) uniform U_GuiInput {
    float debug_float;
    uint debug_bool;
    uint debug_uint;
}
gui_input;
layout(set = 0, binding = 1) uniform U_SunInfo {
    vec3 sun_dir;
    float sun_size;
    vec3 sun_color;
    float sun_luminance;
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

layout(set = 0, binding = 5) uniform U_LavenderInfo {
    float time;
    vec3 bottom_color;
    vec3 tip_color;
}
lavender_info;

layout(set = 0, binding = 6) uniform sampler2D shadow_map_tex_for_vsm_ping;

#include "../include/core/color.glsl"
#include "../include/core/fast_noise_lite.glsl"
#include "../include/core/hash.glsl"
#include "../include/vsm.glsl"
#include "./unpacker.glsl"
#include "./wind.glsl"

const float scaling_factor = 1.0 / 256.0;

void main() {
    ivec3 vox_local_pos;
    uvec3 vert_offset_in_vox;
    float color_gradient;
    float wind_gradient;
    unpack_vertex_data(vox_local_pos, vert_offset_in_vox, color_gradient, wind_gradient, in_packed_data);

    vec3 instance_pos = in_instance_pos * scaling_factor;

    vec3 wind_offset = get_wind_offset(instance_pos.xz, wind_gradient, lavender_info.time);
    vec3 anchor_pos  = (vox_local_pos + wind_offset) * scaling_factor + instance_pos;
    vec3 vert_pos    = anchor_pos + vert_offset_in_vox * scaling_factor;
    vec3 voxel_pos   = anchor_pos + vec3(0.5) * scaling_factor;

    float shadow_weight =
        get_shadow_weight_vsm(shadow_camera_info.view_proj_mat, vec4(voxel_pos, 1.0));

    // transform to clip space
    gl_Position = camera_info.view_proj_mat * vec4(vert_pos, 1.0);

    // interpolate color based on gradient
    vec3 interpolated_color = mix(srgb_to_linear(lavender_info.bottom_color),
                                  srgb_to_linear(lavender_info.tip_color), color_gradient);

    vec3 sun_light = sun_info.sun_color * sun_info.sun_luminance;
    vert_color     = interpolated_color * (sun_light * shadow_weight + shading_info.ambient_light);
}
