#version 450

#extension GL_GOOGLE_include_directive : require

#include "../include/core/packer.glsl"

// these are vertex-rate attributes
layout(location = 0) in uint in_packed_data;

// these are instance-rate attributes (reusing grass instance buffer)
layout(location = 1) in uvec3 in_instance_pos;
layout(location = 2) in uint in_instance_ty;

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

layout(set = 0, binding = 5) uniform U_GrassInfo {
    float time;
    vec3 bottom_color;
    vec3 tip_color;
}
grass_info;

layout(set = 0, binding = 6) uniform sampler2D shadow_map_tex_for_vsm_ping;

#include "../include/core/fast_noise_lite.glsl"
#include "../include/core/hash.glsl"
#include "./unpacker.glsl"

const float scaling_factor = 1.0 / 256.0;

vec2 rand_offset(vec2 instance_pos, float time) {
    const float wind_speed            = 0.6;
    const float wind_strength         = 5.0;
    const float wind_scale            = 2.0;
    const float natual_variance_scale = 1.5;

    // ranges from 0 to 1
    vec2 natual_state = hash_22(instance_pos);
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

    float noise_x = fnlGetNoise2D(state, instance_pos.x + time_offset, instance_pos.y);

    // Sample noise for the Z offset from a different location in the noise field to make it look
    // more natural. Adding a large number to the coordinates ensures we are sampling a different,
    // uncorrelated noise pattern.
    float noise_z =
        fnlGetNoise2D(state, instance_pos.x + 123.4, instance_pos.y - 234.5 + time_offset);

    // The noise is in the range [-1, 1], we scale it by the desired strength.
    return vec2(noise_x, noise_z) * wind_strength + natual_state;
}

vec3 get_wavy_offset(vec2 instance_pos, float gradient) {
    vec2 rand_off = rand_offset(instance_pos, grass_info.time) * gradient * gradient;
    return vec3(rand_off.x, 0.0, rand_off.y);
}

void main() {
    ivec3 vox_local_pos;
    uvec3 vert_offset_in_vox;
    float gradient;
    unpack_vertex_data(vox_local_pos, vert_offset_in_vox, gradient, in_packed_data);

    vec3 instance_pos = in_instance_pos * scaling_factor;

    vec3 wavy_offset = get_wavy_offset(instance_pos.xz, gradient);
    vec3 anchor_pos  = (vox_local_pos + wavy_offset) * scaling_factor + instance_pos;
    vec3 vert_pos    = anchor_pos + vert_offset_in_vox * scaling_factor;

    gl_Position = shadow_camera_info.view_proj_mat * vec4(vert_pos, 1.0);
}
