/// Calculates shadow visibility using Variance Shadow Mapping.
/// Requires:
/// ifndef IGNORE_GET_SHADOW_WEIGHT_VSM: uniform sampler2D vsm_shadow_map_tex;      // RG moments

#ifndef VSM_GLSL
#define VSM_GLSL

const float MIN_VARIANCE = 1e-5;

// for 4xEVSM, these three factors should be enough
const float POSITIVE_EXPONENT = 16.0;
const float NEGATIVE_EXPONENT = 5.0;
const float HARDENING         = 2.0;
// the last factor is for finetuning, it's effective but it came at a cost of harder shadow,
// therefore making the blur kernal less effective
// 0.0: no effect, 1.0: fully black, typically 0.2
const float BLEEDING_REDUCTION_FACTOR = 0.0;

// DO NOT CHANGE THIS VALUE, IT IS THE MAX EXPONENT FOR 32-BIT FLOAT
const float MAX_EXPONENT = 42.0;

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
    float variance = max(moments.y - moments.x * moments.x, MIN_VARIANCE);
    // TODO: float h = 1.0 + shadowMip * 0.5;
    float d = (t - moments.x) * HARDENING;
    return (t <= moments.x) ? 1.0 : variance / (variance + d * d);
}

#ifndef IGNORE_GET_SHADOW_WEIGHT_VSM
float get_shadow_weight_vsm(mat4 shadow_cam_view_proj_mat, vec4 voxel_pos_ws) {
    vec4 light_space = shadow_cam_view_proj_mat * voxel_pos_ws;
    vec3 ndc         = light_space.xyz / light_space.w;
    vec2 uv          = ndc.xy * 0.5 + 0.5;
    float t          = ndc.z; // current depth

    vec2 evsm_depth = warp_depth(t, get_evsm_exponents());
    // sampled from filtered VSM texture
    vec4 occluder = texture(shadow_map_tex_for_vsm_ping, uv);

    float positive_contrib = chebyshev_upper_bound(occluder.xz, evsm_depth.x);
    float negative_contrib = chebyshev_upper_bound(occluder.yw, evsm_depth.y);

    float vis = min(positive_contrib, negative_contrib);

    vis = clamp((vis - BLEEDING_REDUCTION_FACTOR) / (1.0 - BLEEDING_REDUCTION_FACTOR), 0.0, 1.0);

    return vis;
}
#endif // IGNORE_GET_SHADOW_WEIGHT_VSM

#endif // VSM_GLSL
