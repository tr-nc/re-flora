/// Calculates shadow visibility using Variance Shadow Mapping.
/// Requires:
///   uniform sampler2D vsm_shadow_map_tex;      // RG moments

#ifndef VSM_GLSL
#define VSM_GLSL

float linstep(float a, float b, float v) { return clamp((v - a) / (b - a), 0.0, 1.0); }

float reduce_light_bleeding(float pMax, float amount) { return linstep(amount, 1.0, pMax); }

float get_shadow_vsm(mat4 shadow_cam_view_proj_mat, vec4 voxel_pos_ws) {
    vec4 light_space = shadow_cam_view_proj_mat * voxel_pos_ws;
    vec3 ndc         = light_space.xyz / light_space.w;
    vec2 uv          = ndc.xy * 0.5 + 0.5;
    float t          = ndc.z; // current depth

    vec2 moments   = texture(vsm_shadow_map_tex, uv).rg;
    float mean     = moments.x;
    float variance = moments.y - mean * mean;
    variance       = max(variance, 1e-5);

    float dist = t - mean; // distance to mean
    float pMax = variance / (variance + dist * dist);
    pMax       = reduce_light_bleeding(pMax, 0.3);

    // if we are in front of the surface (t ≤ mean) we are definitely lit, otherwise return the
    // probabilistic visibility
    return max(pMax, float(t <= mean));
}

#endif // VSM_GLSL
