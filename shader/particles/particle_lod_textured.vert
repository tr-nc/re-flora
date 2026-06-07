#version 450

#extension GL_GOOGLE_include_directive : require

#include "../foliage/billboard.glsl"
#include "../foliage/unpacker.glsl"
#include "../include/core/color.glsl"
#include "../include/depth_offset.glsl"
#include "../include/sunlight.glsl"

layout(location = 0) in uvec2 in_packed_data;
layout(location = 1) in vec3 in_instance_pos;
layout(location = 2) in float in_instance_size;
layout(location = 3) in vec4 in_instance_color;
layout(location = 4) in uint in_instance_tex_index;

layout(location = 0) out vec4 vert_color;
layout(location = 1) out vec2 vert_uv;
layout(location = 2) flat out uint vert_tex_index;

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
layout(set = 0, binding = 7) uniform sampler2D cloud_shadow_tex;

#define ENABLE_TEMPORAL_VSM
#include "../include/vsm.glsl"
#include "../include/cloud_shadow.glsl"

const uint SPRITE_FLIP_BIT = 0x80000000u;

float get_shadow_weight(ivec3 vox_local_pos) {
    vec3 vox_dir_normalized            = normalize(vec3(vox_local_pos));
    float shadow_negative_side_dropoff = max(0.0, dot(-vox_dir_normalized, sun_info.sun_dir));
    shadow_negative_side_dropoff       = pow(shadow_negative_side_dropoff, 2.0);
    float shadow_weight                = 1.0 - shadow_negative_side_dropoff;

    shadow_weight = max(0.7, shadow_weight);
    return shadow_weight;
}

void main() {
    ivec3 vox_local_pos;
    uvec3 vert_offset_in_vox;
    ivec3 gradient_origin;
    uint max_length;
    unpack_vertex_data(vox_local_pos, vert_offset_in_vox, gradient_origin, max_length,
                       in_packed_data);

    float scale       = max(in_instance_size, 0.001) * 1.25;
    vec3 instance_pos = in_instance_pos;
    vec3 vertex_pos =
        get_vert_pos_with_billboard(camera_info.view_mat, instance_pos, vert_offset_in_vox, scale);

    gl_Position =
        apply_depth_offset(vertex_pos, in_instance_pos, camera_info.view_mat, camera_info.proj_mat);

    float shadow_weight = get_shadow_weight_vsm_temporal(vec4(instance_pos, 1.0));
    shadow_weight *= get_cloud_shadow_transmittance(vec4(instance_pos, 1.0));
    shadow_weight *= get_shadow_weight(vox_local_pos);

    float sun_luminance = sun_luminance_from_dir(sun_info.sun_dir, sun_info.sun_luminance);
    vec3 sun_light      = sun_info.sun_color * sun_luminance;
    vec3 lighting       = sun_light * shadow_weight + shading_info.ambient_light;
    vert_color          = vec4(in_instance_color.rgb * lighting, in_instance_color.a);

    vert_uv            = vec2(float(vert_offset_in_vox.x), 1.0 - float(vert_offset_in_vox.y));
    bool flip_sprite_x = (in_instance_tex_index & SPRITE_FLIP_BIT) != 0u;
    if (flip_sprite_x) {
        vert_uv.x = 1.0 - vert_uv.x;
    }

    vert_tex_index = in_instance_tex_index & ~SPRITE_FLIP_BIT;
}
