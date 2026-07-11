#version 450

#extension GL_GOOGLE_include_directive : require

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_normal;
layout(location = 2) in vec3 in_color_srgb;
layout(location = 3) in vec3 in_base_position;

layout(location = 0) out vec3 vert_color;

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

#include "../include/core/color.glsl"

void main() {
    vec3 world_position = in_base_position + in_position;
    gl_Position = camera_info.view_proj_mat * vec4(world_position, 1.0);

    vec3 normal = normalize(in_normal);
    float facing = max(dot(normal, normalize(sun_info.sun_dir)), 0.0);
    float wrap_light = 0.28 + 0.72 * facing;
    vec3 sunlight = sun_info.sun_color * sun_info.sun_luminance * wrap_light;
    vec3 base_color = srgb_to_linear(in_color_srgb);
    vert_color = base_color * (shading_info.ambient_light + sunlight);
}
