/// Calculates shadow visibility using Variance Shadow Mapping.
/// Requires:
///   uniform sampler2D vsm_shadow_map_tex;      // RG moments

#ifndef VSM_GLSL
#define VSM_GLSL

const float MIN_VARIANCE                 = 1e-5;
const float REDUCE_LIGHT_BLEEDING_AMOUNT = 0.3;

float linstep(float a, float b, float v) { return clamp((v - a) / (b - a), 0.0, 1.0); }

float reduce_light_bleeding(float p_max, float amount) { return linstep(amount, 1.0, p_max); }

float chebyshev_upper_bound(vec2 moments, float t) {
    float variance = moments.y - moments.x * moments.x;
    variance       = max(variance, MIN_VARIANCE);
    float d        = t - moments.x;

    float p_max = variance / (variance + d * d);
    p_max       = reduce_light_bleeding(p_max, REDUCE_LIGHT_BLEEDING_AMOUNT);

    return (t <= moments.x ? 1.0f : p_max);
}

float get_shadow_vsm(mat4 shadow_cam_view_proj_mat, vec4 voxel_pos_ws) {
    vec4 light_space = shadow_cam_view_proj_mat * voxel_pos_ws;
    vec3 ndc         = light_space.xyz / light_space.w;
    vec2 uv          = ndc.xy * 0.5 + 0.5;
    float t          = ndc.z; // current depth

    vec2 moments = texture(vsm_shadow_map_tex, uv).rg;
    return chebyshev_upper_bound(moments, t);
}

#endif // VSM_GLSL
