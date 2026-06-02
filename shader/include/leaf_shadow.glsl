#ifndef LEAF_SHADOW_GLSL
#define LEAF_SHADOW_GLSL

// Separate moving-leaf shadow path.
// Requires:
//   uniform sampler2D leaf_shadow_opacity_tex; // alpha = accumulated leaf opacity
//   uniform sampler2D leaf_shadow_mask_tex;    // alpha = conservative low-res influence mask
//   U_ShadowCameraInfo shadow_camera_info;

const float LEAF_SHADOW_STRENGTH = 0.62;
const float LEAF_SHADOW_MIN_TRANSMITTANCE = 0.42;
const float LEAF_SHADOW_MASK_SAMPLE_THRESHOLD = 0.01;

float sample_leaf_shadow_opacity_pcf(vec2 uv) {
    vec2 texel = 1.0 / vec2(textureSize(leaf_shadow_opacity_tex, 0));
    float opacity = 0.0;
    opacity += texture(leaf_shadow_opacity_tex, uv).a * 0.40;
    opacity += texture(leaf_shadow_opacity_tex, uv + vec2(texel.x, 0.0)).a * 0.15;
    opacity += texture(leaf_shadow_opacity_tex, uv - vec2(texel.x, 0.0)).a * 0.15;
    opacity += texture(leaf_shadow_opacity_tex, uv + vec2(0.0, texel.y)).a * 0.15;
    opacity += texture(leaf_shadow_opacity_tex, uv - vec2(0.0, texel.y)).a * 0.15;
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
    float transmittance = 1.0 - opacity * LEAF_SHADOW_STRENGTH;
    return clamp(transmittance, LEAF_SHADOW_MIN_TRANSMITTANCE, 1.0);
}

#endif // LEAF_SHADOW_GLSL
