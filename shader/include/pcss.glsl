/// Calculates shadow visibility using Percentage Closer Soft Shadowing.
/// Requires:
/// uniform sampler2D shadow_map_tex;
#ifndef PCSS_GLSL
#define PCSS_GLSL

const float SHADOW_EPSILON = 1e-2;

const int BLOCKER_HALF_SIZE   = 3;
const float BLOCKER_UV_OFFSET = 1.0 / 1024.0;

const int KERNAL_HALF_SIZE = 1;
const float LIGHT_SIZE_UV  = 0.002;

float get_avg_blocker_depth(vec2 base_uv, float ref_z) {
    float blocker_sum = 0.0;
    int blocker_cnt   = 0;

    for (int y = -BLOCKER_HALF_SIZE; y <= BLOCKER_HALF_SIZE; ++y) {
        for (int x = -BLOCKER_HALF_SIZE; x <= BLOCKER_HALF_SIZE; ++x) {
            vec2 offset    = vec2(x, y) * BLOCKER_UV_OFFSET;
            float sample_z = texture(shadow_map_tex, base_uv + offset).r;

            if (sample_z + SHADOW_EPSILON < ref_z) { // sample is an occluder
                blocker_sum += sample_z;
                ++blocker_cnt;
            }
        }
    }

    return (blocker_cnt > 0) ? blocker_sum / float(blocker_cnt) : -1.0;
}

float get_shadow_weight(vec3 voxel_pos_ws) {
    vec4 light_space = shadow_camera_info.view_proj_mat * vec4(voxel_pos_ws, 1.0);
    vec3 ndc         = light_space.xyz / light_space.w; // NDC
    vec2 base_uv     = ndc.xy * 0.5 + 0.5;              // in [0,1]^2
    float ref_z      = ndc.z;                           // depth of current fragment

    float avg_blocker_z = get_avg_blocker_depth(base_uv, ref_z);

    if (avg_blocker_z < 0.0) return 1.0;

    // penumbra radius
    float penumbra_uv = (ref_z - avg_blocker_z) / avg_blocker_z * LIGHT_SIZE_UV;

    float uv_step = penumbra_uv;

    const float sigma      = float(KERNAL_HALF_SIZE) / 3.0;
    const float two_sigma2 = 2.0 * sigma * sigma;

    float weighted_vis_sum = 0.0;
    float weight_sum       = 0.0;

    for (int y = -KERNAL_HALF_SIZE; y <= KERNAL_HALF_SIZE; ++y) {
        for (int x = -KERNAL_HALF_SIZE; x <= KERNAL_HALF_SIZE; ++x) {
            vec2 offset    = vec2(x, y);
            float gaussian = exp(-dot(offset, offset) / two_sigma2); // weight

            float sample_z = texture(shadow_map_tex, base_uv + offset * uv_step).r;

            float vis = (ref_z - SHADOW_EPSILON < sample_z) ? 1.0 : 0.0;

            weighted_vis_sum += gaussian * vis;
            weight_sum += gaussian;
        }
    }

    return weighted_vis_sum / weight_sum;
    // return avg_blocker_z;
}

#endif // PCSS_GLSL
