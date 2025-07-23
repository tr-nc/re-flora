#version 450

#extension GL_GOOGLE_include_directive : require

#include "../include/core/packer.glsl"

// these are vertex-rate attributes
layout(location = 0) in uint in_packed_data;

// these are instance-rate attributes (reusing grass instance buffer)
layout(location = 1) in uvec3 in_instance_position;
layout(location = 2) in uint in_instance_grass_type;

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

layout(set = 0, binding = 5) uniform U_LeafInfo {
    float time;
    vec3 bottom_color;
    vec3 tip_color;
}
leaf_info;

layout(set = 0, binding = 6) uniform sampler2D shadow_map_tex_for_vsm_ping;

#include "../include/core/hash.glsl"
#include "../include/vsm.glsl"
#include "./unpacker.glsl"

const float scaling_factor = 1.0 / 256.0;

void unpack_vertex_data(out vec3 o_base_pos, out uint o_vertex_offset, out float o_gradient,
                        uint packed_data) {
    // Extract base position (24 bits: 8 bits each for x, y, z)
    o_base_pos = vec3(packed_data & 0xFF, (packed_data >> 8) & 0xFF, (packed_data >> 16) & 0xFF);
    // Extract vertex offset (3 bits)
    o_vertex_offset = (packed_data >> 24) & 0x7u;
    // Extract gradient (5 bits)
    o_gradient = float((packed_data >> 27) & 0x1F) / 31.0;
}

// Calculate normal for a cube face based on vertex position
vec3 calculate_cube_normal(vec3 vertex_pos, vec3 base_pos) {
    vec3 offset    = vertex_pos - base_pos;
    vec3 center    = base_pos + vec3(0.5);
    vec3 to_vertex = normalize(vertex_pos + vec3(0.5) - center);

    // Determine which face this vertex belongs to
    vec3 abs_offset = abs(to_vertex);
    if (abs_offset.x > abs_offset.y && abs_offset.x > abs_offset.z) {
        return vec3(sign(to_vertex.x), 0, 0);
    } else if (abs_offset.y > abs_offset.z) {
        return vec3(0, sign(to_vertex.y), 0);
    } else {
        return vec3(0, 0, sign(to_vertex.z));
    }
}

void main() {
    ivec3 vox_local_pos;
    uvec3 vert_offset_in_vox;
    float color_gradient;
    unpack_vertex_data(vox_local_pos, vert_offset_in_vox, color_gradient, in_packed_data);
    vox_local_pos = vox_local_pos - ivec3(128);

    vec3 instance_pos = in_instance_position * scaling_factor;

    vec3 wavy_offset = vec3(0.0);
    vec3 anchor_pos  = (vox_local_pos + wavy_offset) * scaling_factor + instance_pos;
    vec3 vert_pos    = anchor_pos + vert_offset_in_vox * scaling_factor;
    vec3 voxel_pos   = anchor_pos + vec3(0.5) * scaling_factor;

    float shadow_weight =
        get_shadow_weight_vsm(shadow_camera_info.view_proj_mat, vec4(voxel_pos, 1.0));

    // transform to clip space
    gl_Position = camera_info.view_proj_mat * vec4(vert_pos, 1.0);

    // interpolate color based on color gradient
    vec3 interpolated_color = mix(leaf_info.bottom_color, leaf_info.tip_color, color_gradient);

    vec3 sun_light = sun_info.sun_color * sun_info.sun_luminance;
    vert_color     = interpolated_color * (sun_light * shadow_weight + shading_info.ambient_light);
}
