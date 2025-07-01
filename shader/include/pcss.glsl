/// Calculates shadow visibility using Percentage Closer Soft Shadowing.
/// Requires:
/// uniform sampler2D shadow_map_tex;

#ifndef PCSS_GLSL
#define PCSS_GLSL

// Gaussian Percentage-Closer Soft Shadowing
// ----------------------------------------
const int KERNAL_HALF_SIZE = 4;            // compile-time: choose any size
const float UV_OFFSET      = 0.4 / 1024.0; // 1 / shadowMapResolution
const float SHADOW_EPSILON = 1e-3;         // depth bias to avoid precision artefacts

float get_shadow_weight(vec3 voxel_pos_ws) {
    // -------------------------------------------------------------
    // 1. Transform world-space position to light clip space
    // -------------------------------------------------------------
    vec4 light_space = shadow_camera_info.view_proj_mat * vec4(voxel_pos_ws, 1.0);
    vec3 ndc         = light_space.xyz / light_space.w; // Normalised device coords
    vec2 base_uv     = ndc.xy * 0.5 + 0.5;              // [0,1]²
    float ref_z      = ndc.z;                           // depth of current fragment

    // -------------------------------------------------------------
    // 2. Gaussian parameters – build on the fly
    //    ±KERNAL_HALF_SIZE  ≅  3·σ  (≈99.7 % of energy)
    // -------------------------------------------------------------
    const float sigma      = float(KERNAL_HALF_SIZE) / 3.0;
    const float two_sigma2 = 2.0 * sigma * sigma;

    float weighted_vis_sum = 0.0; // Σ w · vis
    float weight_sum       = 0.0; // Σ w

    // -------------------------------------------------------------
    // 3. Filter taps
    // -------------------------------------------------------------
    for (int y = -KERNAL_HALF_SIZE; y <= KERNAL_HALF_SIZE; ++y) {
        for (int x = -KERNAL_HALF_SIZE; x <= KERNAL_HALF_SIZE; ++x) {
            vec2 offset = vec2(x, y);
            float w     = exp(-dot(offset, offset) / two_sigma2); // Gaussian weight

            float sample_z = texture(shadow_map_tex, base_uv + offset * UV_OFFSET).r;

            // Visibility test with ε bias
            float vis = (ref_z - SHADOW_EPSILON < sample_z) ? 1.0 : 0.0;

            weighted_vis_sum += w * vis;
            weight_sum += w;
        }
    }

    // -------------------------------------------------------------
    // 4. Normalise to [0,1] and return
    // -------------------------------------------------------------
    return weighted_vis_sum / weight_sum;
}

#endif // PCSS_GLSL
