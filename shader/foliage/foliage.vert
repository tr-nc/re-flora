#version 450

#extension GL_GOOGLE_include_directive : require

// these are vertex-rate attributes
layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_color;
layout(location = 2) in uint in_height; // The voxel's stack level

// these are instance-rate attributes
// this should match the grass_instance.glsl
layout(location = 3) in uvec3 in_instance_position;
layout(location = 4) in uint in_instance_grass_type;

layout(location = 0) out vec3 vert_color;

layout(set = 0, binding = 0) uniform U_CameraInfo {
    vec4 pos;
    mat4 view_mat;
    mat4 view_mat_inv;
    mat4 proj_mat;
    mat4 proj_mat_inv;
    mat4 view_proj_mat;
    mat4 view_proj_mat_inv;
}
camera_info;

layout(set = 0, binding = 1) uniform U_ShadowCameraInfo {
    vec4 pos;
    mat4 view_mat;
    mat4 view_mat_inv;
    mat4 proj_mat;
    mat4 proj_mat_inv;
    mat4 view_proj_mat;
    mat4 view_proj_mat_inv;
}
shadow_camera_info;

layout(set = 0, binding = 2) uniform U_GrassInfo { float time; }
grass_info;

layout(set = 0, binding = 3) uniform sampler2D shadow_map_tex;

#include "../include/core/fast_noise_lite.glsl"
#include "../include/core/hash.glsl"

const uint voxel_count     = 8;
const float scaling_factor = 1.0 / 256.0;

vec3 get_offset_of_vertex(float voxel_height, uint voxel_count, vec2 grass_offset) {
    // Avoid division by zero if the blade has only one voxel.
    float denom = float(max(voxel_count - 1u, 1u));

    float t       = voxel_height / denom;
    float t_curve = t * t; // ease-in curve for a natural bend

    // Calculate the floating-point center of the voxel based on its height and bend
    return vec3(grass_offset.x * t_curve, 0.0, grass_offset.y * t_curve);
}

// Simplified version that assumes the sample is always valid.
float get_shadow_weight(vec4 voxel_pos_ws) {
    vec4 point_ndc = shadow_camera_info.view_proj_mat * voxel_pos_ws;
    vec2 shadow_uv = point_ndc.xy / point_ndc.w * 0.5 + 0.5;

    float shadow_depth = texture(shadow_map_tex, shadow_uv).r;
    float depth_01     = point_ndc.z / point_ndc.w;
    float delta        = depth_01 - shadow_depth;
    bool is_in_shadow  = delta > 0.001;

    return is_in_shadow ? 0.0 : 1.0;
}

// Optimized version assuming all samples are valid.
float get_shadow_weight_soft(vec4 voxel_pos_ws) {
    // The 1D binomial kernel weights are [1, 2, 1]. The total 3D weight is 4*4*4 = 64.
    const float total_weight = 64.0;
    const float weights[3]   = float[](1.0, 2.0, 1.0);

    float accumulated_shadow = 0.0;

    for (int x = -1; x <= 1; x++) {
        for (int y = -1; y <= 1; y++) {
            for (int z = -1; z <= 1; z++) {
                float sample_weight = weights[x + 1] * weights[y + 1] * weights[z + 1];
                vec3 offset         = vec3(x, y, z) * scaling_factor;

                float shadow_value = get_shadow_weight(voxel_pos_ws + vec4(offset, 0.0));

                accumulated_shadow += shadow_value * sample_weight;
            }
        }
    }

    return accumulated_shadow / total_weight;
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
    vec3 voxel_pos_ms  = float(in_height) + vec3(0.5) + vertex_offset;
    vec4 voxel_pos_ws  = vec4(voxel_pos_ms + in_instance_position, 1.0);

    mat4 scale_mat  = mat4(1.0);
    scale_mat[0][0] = scaling_factor;
    scale_mat[1][1] = scaling_factor;
    scale_mat[2][2] = scaling_factor;
    vert_pos_ws     = (scale_mat * vert_pos_ws);
    voxel_pos_ws    = (scale_mat * voxel_pos_ws);

    float shadow_weight = get_shadow_weight_soft(voxel_pos_ws);

    // transform to clip space
    gl_Position = camera_info.view_proj_mat * vert_pos_ws;

    float ambient_light = 0.2;
    vert_color          = in_color * (shadow_weight + ambient_light);
}
