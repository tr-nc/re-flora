#version 450

#extension GL_GOOGLE_include_directive : require

#include "../include/core/packer.glsl"

// these are vertex-rate attributes
layout(location = 0) in uint in_packed_data;

// these are instance-rate attributes (reusing leaves instance buffer)
layout(location = 1) in uvec3 in_instance_position;
layout(location = 2) in uint in_instance_leaves_type;

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

layout(set = 0, binding = 5) uniform U_LeavesInfo {
    float time;
    vec3 bottom_color;
    vec3 tip_color;
}
leaves_info;

layout(set = 0, binding = 6) uniform sampler2D shadow_map_tex_for_vsm_ping;

#include "../include/core/fast_noise_lite.glsl"
#include "../include/core/hash.glsl"
#include "../include/vsm.glsl"
#include "./unpacker.glsl"

const float scaling_factor = 1.0 / 256.0;

float get_shadow_weight(ivec3 vox_local_pos) {
    vec3 vox_dir_normalized            = normalize(vec3(vox_local_pos));
    float shadow_negative_side_dropoff = max(0.0, dot(-vox_dir_normalized, sun_info.sun_dir));
    shadow_negative_side_dropoff       = pow(shadow_negative_side_dropoff, 2.0);
    float shadow_weight                = 1.0 - shadow_negative_side_dropoff;

    shadow_weight = max(0.7, shadow_weight);
    return shadow_weight;
}

vec2 random_leaves_offset(vec2 leaves_instance_pos, float time) {
    const float wind_speed    = 0.6; // how fast the wind moves
    const float wind_strength = 5.0; // how much the leaves offsets
    const float wind_scale    = 2.0; // the size of the wind gusts. smaller value = larger gusts.
    const float natual_variance_scale = 1.5; // how much the leaves varies naturally

    // ranges from 0 to 1
    vec2 natual_state = hash_22(leaves_instance_pos);
    // convert to -1 to 1
    natual_state = natual_state * 2.0 - 1.0;
    natual_state = natual_state * natual_variance_scale;

    fnl_state state    = fnlCreateState(469);
    state.noise_type   = FNL_NOISE_PERLIN;
    state.fractal_type = FNL_FRACTAL_FBM;
    state.frequency    = wind_scale;
    state.octaves      = 2;
    state.lacunarity   = 2.0;
    state.gain         = 0.2;

    float time_offset = time * wind_speed;

    float noise_x =
        fnlGetNoise2D(state, leaves_instance_pos.x + time_offset, leaves_instance_pos.y);

    // Sample noise for the Z offset from a different location in the noise field to make it look
    // more natural. Adding a large number to the coordinates ensures we are sampling a different,
    // uncorrelated noise pattern.
    float noise_z = fnlGetNoise2D(state, leaves_instance_pos.x + 123.4,
                                  leaves_instance_pos.y - 234.5 + time_offset);

    // The noise is in the range [-1, 1], we scale it by the desired strength.
    return vec2(noise_x, noise_z) * wind_strength + natual_state;
}

vec3 get_wavy_offset(float bend_factor, vec2 leaves_instance_pos) {
    vec2 leaves_offset = random_leaves_offset(leaves_instance_pos, leaves_info.time);
    float t_curve      = bend_factor * bend_factor;
    return vec3(leaves_offset.x * t_curve, 0.0, leaves_offset.y * t_curve);
}

void main() {
    ivec3 vox_local_pos;
    uvec3 vert_offset_in_vox;
    float color_gradient;
    unpack_vertex_data(vox_local_pos, vert_offset_in_vox, color_gradient, in_packed_data);
    vox_local_pos = vox_local_pos - ivec3(128);

    vec3 instance_pos = in_instance_position * scaling_factor;

    // TODO: make 5.0 configurable inside leaves_info
    vec3 wavy_offset =
        get_wavy_offset(distance(vox_local_pos, vec3(0.0)) / 128.0 * 5.0, instance_pos.xz);
    vec3 anchor_pos = (vox_local_pos + wavy_offset) * scaling_factor + instance_pos;
    vec3 vert_pos   = anchor_pos + vert_offset_in_vox * scaling_factor;
    vec3 voxel_pos  = anchor_pos + vec3(0.5) * scaling_factor;

    // get_shadow_weight_vsm creates unstable very unstable shadows when the sun changes direction
    float shadow_weight = get_shadow_weight(vox_local_pos);

    // transform to clip space
    gl_Position = camera_info.view_proj_mat * vec4(vert_pos, 1.0);

    // interpolate color based on color gradient
    vec3 interpolated_color = mix(leaves_info.bottom_color, leaves_info.tip_color, color_gradient);

    vec3 sun_light = sun_info.sun_color * sun_info.sun_luminance;
    vert_color     = interpolated_color * (sun_light * shadow_weight + shading_info.ambient_light);
}
