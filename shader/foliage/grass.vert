#version 450

#extension GL_GOOGLE_include_directive : require

// these are vertex-rate attributes
layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_color;
layout(location = 2) in uint in_height; // The voxel's stack level

// these are instance-rate attributes
layout(location = 3) in uvec3 in_instance_position;
layout(location = 4) in uint in_instance_grass_type;

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
layout(set = 0, binding = 2) uniform U_CameraInfo {
    vec4 pos;
    mat4 view_mat;
    mat4 view_mat_inv;
    mat4 proj_mat;
    mat4 proj_mat_inv;
    mat4 view_proj_mat;
    mat4 view_proj_mat_inv;
}
camera_info;

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

layout(set = 0, binding = 4) uniform U_GrassInfo { float time; }
grass_info;

layout(set = 0, binding = 5) uniform sampler2D vsm_shadow_map_tex;

#include "../include/core/fast_noise_lite.glsl"
#include "../include/core/hash.glsl"
// #include "../include/pcss.glsl"
#include "../include/vsm.glsl"

const uint voxel_count     = 8;
const float scaling_factor = 1.0 / 256.0;

vec3 get_offset_of_vertex(float voxel_height, uint voxel_count, vec2 grass_offset) {
    float denom   = float(max(voxel_count - 1u, 1u));
    float t       = voxel_height / denom;
    float t_curve = t * t;
    return vec3(grass_offset.x * t_curve, 0.0, grass_offset.y * t_curve);
}

vec2 random_grass_offset(vec2 grass_instance_pos, float time) {
    const float wind_speed    = 0.6; // how fast the wind moves
    const float wind_strength = 5.0; // how much the grass bends
    const float wind_scale    = 2.0; // the size of the wind gusts. smaller value = larger gusts.
    const float natual_variance_scale = 1.5; // how much the grass varies naturally

    // ranges from 0 to 1
    vec2 natual_state = hash_22(grass_instance_pos);
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

    float noise_x = fnlGetNoise2D(state, grass_instance_pos.x + time_offset, grass_instance_pos.y);

    // Sample noise for the Z offset from a different location in the noise field to make it look
    // more natural. Adding a large number to the coordinates ensures we are sampling a different,
    // uncorrelated noise pattern.
    float noise_z = fnlGetNoise2D(state, grass_instance_pos.x + 123.4,
                                  grass_instance_pos.y - 234.5 + time_offset);

    // The noise is in the range [-1, 1], we scale it by the desired strength.
    return vec2(noise_x, noise_z) * wind_strength + natual_state;
}

void main() {
    float height = float(in_height);

    vec2 grass_offset =
        random_grass_offset(vec2(in_instance_position.xz * scaling_factor), grass_info.time);

    vec3 vertex_offset = get_offset_of_vertex(height, voxel_count, grass_offset);
    vec3 vert_pos_ms   = in_position + vertex_offset;
    vec4 vert_pos_ws   = vec4(vert_pos_ms + in_instance_position, 1.0);
    vec3 voxel_pos_ms  = vec3(0.0, in_height, 0.0) + vec3(0.5) + vertex_offset;
    vec4 voxel_pos_ws  = vec4(voxel_pos_ms + in_instance_position, 1.0);

    mat4 scale_mat  = mat4(1.0);
    scale_mat[0][0] = scaling_factor;
    scale_mat[1][1] = scaling_factor;
    scale_mat[2][2] = scaling_factor;
    vert_pos_ws     = (scale_mat * vert_pos_ws);
    voxel_pos_ws    = (scale_mat * voxel_pos_ws);

    float shadow_weight = get_shadow_weight_vsm(shadow_camera_info.view_proj_mat, voxel_pos_ws);

    // transform to clip space
    gl_Position = camera_info.view_proj_mat * vert_pos_ws;

    float ambient_light = 0.3;
    vec3 sun_light = sun_info.sun_color * sun_info.sun_luminance;
    vert_color = in_color * (sun_light * shadow_weight + ambient_light);
}
