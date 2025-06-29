/// Calculates shadow visibility using Variance Shadow Mapping.
/// Requires: uniform sampler2D vsm_shadow_map_tex

#ifndef VSM_GLSL
#define VSM_GLSL

// Calculates shadow visibility using Variance Shadow Mapping.
// Returns a value from 0.0 (in shadow) to 1.0 (fully lit).
// https://www.shadertoy.com/view/MlKSRm
// https://developer.download.nvidia.com/SDK/10/direct3d/Source/VarianceShadowMapping/Doc/VarianceShadowMapping.pdf
float get_shadow_vsm(mat4 shadow_cam_view_proj_mat, vec4 voxel_pos_ws) {
    vec4 point_light_space = shadow_cam_view_proj_mat * voxel_pos_ws;

    vec3 point_ndc = point_light_space.xyz / point_light_space.w;
    vec2 shadow_uv = point_ndc.xy * 0.5 + 0.5;

    float t = point_ndc.z;

    vec2 moments = texture(vsm_shadow_map_tex, shadow_uv).rg;
    float ex     = moments.x;
    float ex_2   = moments.y;

    float variance = ex_2 - ex * ex;

    float znorm   = t - ex;
    float znorm_2 = znorm * znorm;

    float p = variance / (variance + znorm_2);

    return max(p, float(t <= ex));
}

#endif // VSM_GLSL
