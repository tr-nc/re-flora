/// Calculates shadow visibility using Variance Shadow Mapping.
/// Requires:
/// ifndef IGNORE_GET_SHADOW_VSM: uniform sampler2D vsm_shadow_map_tex;      // RG moments

#ifndef VSM_GLSL
#define VSM_GLSL

const float MIN_VARIANCE = 1e-5;

const float POSITIVE_EXPONENT = 30.0;
const float NEGATIVE_EXPONENT = 5.0;
// DO NOT CHANGE THIS VALUE, IT IS THE MAX EXPONENT FOR 32-BIT FLOAT
const float MAX_EXPONENT = 42.0f;

vec2 get_evsm_exponents() {
    vec2 exponents = vec2(POSITIVE_EXPONENT, NEGATIVE_EXPONENT);
    return min(exponents, MAX_EXPONENT);
}

// 对阴影贴图深度应用指数扭曲，输入深度应为[0,1]
vec2 warp_depth(float depth, vec2 exponents) {
    // Rescale depth into [-1, 1]
    depth     = 2.0f * depth - 1.0f;
    float pos = exp(exponents.x * depth);
    float neg = -exp(-exponents.y * depth);
    return vec2(pos, neg);
}

float chebyshev_upper_bound(vec2 moments, float t) {
    float variance = moments.y - moments.x * moments.x;
    variance       = max(variance, MIN_VARIANCE);
    float d        = t - moments.x;

    float p_max = variance / (variance + d * d);

    return (t <= moments.x ? 1.0f : p_max);
}

#ifndef IGNORE_GET_SHADOW_VSM
float get_shadow_vsm(mat4 shadow_cam_view_proj_mat, vec4 voxel_pos_ws) {
    vec4 light_space = shadow_cam_view_proj_mat * voxel_pos_ws;
    vec3 ndc         = light_space.xyz / light_space.w;
    vec2 uv          = ndc.xy * 0.5 + 0.5;
    float t          = ndc.z; // current depth

    vec2 evsm_depth = warp_depth(t, get_evsm_exponents());
    // sampled from filtered VSM texture
    vec4 occluder = texture(vsm_shadow_map_tex, uv);

    float positive_contrib = chebyshev_upper_bound(occluder.xz, evsm_depth.x);
    float negative_contrib = chebyshev_upper_bound(occluder.yw, evsm_depth.y);

    return min(positive_contrib, negative_contrib);
}
#endif // IGNORE_GET_SHADOW_VSM

#endif // VSM_GLSL
