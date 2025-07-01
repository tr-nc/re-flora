/// Calculates shadow visibility using Percentage Closer Soft Shadowing.
/// Requires:
/// uniform sampler2D shadow_map_tex;
#ifndef PCSS_GLSL
#define PCSS_GLSL

#include "./core/definitions.glsl"
#include "./core/sampling.glsl"
#include "./noise_tex.glsl"

float SHADOW_EPSILON = 3.5 * 1e-3;

float BLOCKER_SEARCH_WIDTH = 2.0 / 100.0;
float LIGHT_SIZE_UV        = 4.0 / 100.0;

const float NEAR_PLANE = 0.01;

const int BLOCKER_SEARCH_SAMPLE_COUNT = 16;
const int PCF_SAMPLE_COUNT            = 16;

mat2 get_rotation_matrix(float random_01) {
    float angle = random_01 * TWO_PI;
    float c     = cos(angle);
    float s     = sin(angle);
    return mat2(c, -s, s, c);
}

// https://github.com/proskur1n/vwa-code/blob/master/source/shaders/normalPass.frag
float pcf(vec2 base_uv, float ref_z, float filter_radius, ivec3 seed) {
    float random_01 = random_float_bn(seed);
    mat2 rot        = get_rotation_matrix(random_01);

    float sum_of_weight          = 0.0;
    float sum_of_weighted_values = 0.0;
    for (int i = 0; i < PCF_SAMPLE_COUNT; ++i) {
        vec2 offset = rot * POISSON_16[i] * filter_radius;

        float sample_z = texture(shadow_map_tex, base_uv + offset).r;
        float vis      = (sample_z + SHADOW_EPSILON > ref_z) ? 1.0 : 0.0;

        float weight = 1.0;

        sum_of_weight += weight;
        sum_of_weighted_values += weight * vis;
    }
    return sum_of_weighted_values / sum_of_weight;
}

float get_avg_blocker_depth(vec2 base_uv, float ref_z, ivec3 seed) {
    float search_width = BLOCKER_SEARCH_WIDTH;

    float random_01 = random_float_bn(seed);
    mat2 rot        = get_rotation_matrix(random_01);

    float blocker_sum = 0.0;
    int blocker_cnt   = 0;
    for (int i = 0; i < BLOCKER_SEARCH_SAMPLE_COUNT; ++i) {
        vec2 offset    = rot * POISSON_16[i] * search_width;
        float sample_z = texture(shadow_map_tex, base_uv + offset).r;

        if (sample_z + SHADOW_EPSILON < ref_z) { // sample is an occluder
            blocker_sum += sample_z;
            ++blocker_cnt;
        }
    }
    return (blocker_cnt > 0) ? blocker_sum / float(blocker_cnt) : -1.0;
}

float get_shadow_weight_pcss(vec3 voxel_pos_ws, ivec3 seed) {
    vec4 light_space = shadow_camera_info.view_proj_mat * vec4(voxel_pos_ws, 1.0);
    vec3 ndc         = light_space.xyz / light_space.w; // NDC
    vec2 base_uv     = ndc.xy * 0.5 + 0.5;              // in [0,1]^2
    float ref_z      = ndc.z;                           // depth of current fragment

    float avg_blocker_z = get_avg_blocker_depth(base_uv, ref_z, seed);

    if (avg_blocker_z < 0.0) return 1.0;

    float penumbra_filter_radius = (ref_z - avg_blocker_z) * LIGHT_SIZE_UV;

    return pcf(base_uv, ref_z, penumbra_filter_radius, seed);
}

#endif // PCSS_GLSL
