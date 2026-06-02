#ifndef LEAF_SHADOW_GLSL
#define LEAF_SHADOW_GLSL

// Separate moving-leaf shadow path.
// Requires:
//   uniform sampler2D leaf_shadow_opacity_blended_tex; // alpha = accumulated leaf opacity
//   uniform sampler2D leaf_shadow_mask_tex;            // alpha = conservative low-res influence mask
//   U_ShadowCameraInfo shadow_camera_info;

const float LEAF_SHADOW_MASK_SAMPLE_THRESHOLD = 0.01;

float sample_leaf_shadow_opacity_pcf(vec2 uv) {
    vec2 texel = 1.0 / vec2(textureSize(leaf_shadow_opacity_blended_tex, 0));
    vec2 radius = texel * max(gui_input.leaf_shadow_filter_radius_texels, 0.0);
    float opacity = texture(leaf_shadow_opacity_blended_tex, uv).a;

    // The opacity map is intentionally high-res and leaf voxels are small, so an
    // averaging PCF can make single-leaf silhouettes disappear. Use a small
    // conservative max filter instead; the low-res mask has already limited this
    // path to leaf-shadow regions.
    for (int y = -1; y <= 1; y++) {
        for (int x = -1; x <= 1; x++) {
            vec2 offset = vec2(float(x), float(y)) * radius;
            opacity = max(opacity, texture(leaf_shadow_opacity_blended_tex, uv + offset).a);
        }
    }

    return clamp(opacity, 0.0, 1.0);
}

float get_leaf_shadow_transmittance(vec4 voxel_pos_ws, bool receiver_accepts_leaf_shadow) {
    if (!receiver_accepts_leaf_shadow) {
        return 1.0;
    }

    vec4 light_space = shadow_camera_info.view_proj_mat * voxel_pos_ws;
    vec3 ndc = light_space.xyz / light_space.w;
    vec2 uv = ndc.xy * 0.5 + 0.5;

    if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) {
        return 1.0;
    }

    float mask = texture(leaf_shadow_mask_tex, uv).a;
    if (mask < LEAF_SHADOW_MASK_SAMPLE_THRESHOLD) {
        return 1.0;
    }

    float opacity = sample_leaf_shadow_opacity_pcf(uv) * mask;
    float strength = max(gui_input.leaf_shadow_strength, 0.0);
    float min_transmittance = clamp(gui_input.leaf_shadow_min_transmittance, 0.0, 1.0);
    float transmittance = 1.0 - opacity * strength;
    return clamp(transmittance, min_transmittance, 1.0);
}

#endif // LEAF_SHADOW_GLSL
