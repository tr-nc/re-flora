#version 450

#extension GL_GOOGLE_include_directive : require

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_voxel_center;
layout(location = 2) in vec3 in_shading_normal;
layout(location = 3) in vec3 in_color_srgb;
layout(location = 4) in vec3 in_animation_direction;
layout(location = 5) in vec3 in_base_position;

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

#include "../foliage/flora_animation_info.glsl"
#include "../include/core/color.glsl"
#include "../include/sunlight.glsl"
#define ENABLE_TEMPORAL_VSM
#include "../include/vsm.glsl"
#include "../include/leaf_shadow.glsl"
#include "../include/cloud_shadow.glsl"
#define DIRECT_SUN_SHADOW_ENABLE_LEAF
#define DIRECT_SUN_SHADOW_ENABLE_CLOUD
#include "../include/direct_sun_shadow.glsl"
#include "../include/stylized_voxel_lighting.glsl"
#include "../include/terrain_edit_preview.glsl"

const float CAP_MOTION_SECONDS = 1.0;
const float ANIMATION_DISTANCE = 1.0 / 256.0;

float sprinkler_extension() {
    float tick_seconds = max(gui_input.world_tick_seconds, 1.0 / 240.0);
    uint half_cycle_ticks = max(uint(round((CAP_MOTION_SECONDS * 0.5) / tick_seconds)), 1u);
    uint cycle_ticks = half_cycle_ticks * 2u;
    uint cycle_tick = flora_growth_info.flora_tick % cycle_ticks;
    float phase = float(cycle_tick) / float(cycle_ticks);
    return 0.5 - 0.5 * cos(phase * 6.28318530718);
}

void main() {
    float extension = sprinkler_extension();
    vec3 animation_offset = in_animation_direction * extension * ANIMATION_DISTANCE;
    vec3 world_position = in_base_position + in_position + animation_offset;
    vec3 voxel_center = in_base_position + in_voxel_center + animation_offset;
    gl_Position = camera_info.view_proj_mat * vec4(world_position, 1.0);

    vec3 base_color = srgb_to_linear(in_color_srgb);
    float shadow_weight = stylized_voxel_shadow_weight(voxel_center, in_shading_normal);
    vec3 lit_color = apply_stylized_voxel_lighting(base_color, shadow_weight);
    vert_color = apply_terrain_edit_preview_tint(lit_color, voxel_center);
}
