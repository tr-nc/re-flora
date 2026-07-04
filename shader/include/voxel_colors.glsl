#ifndef VOXEL_COLORS_GLSL
#define VOXEL_COLORS_GLSL

#include "./core/color.glsl"
#include "./voxel_types.glsl"

vec3 _voxel_color_by_type_srgb(uint voxel_type) {
    if (voxel_type == VOXEL_TYPE_EMPTY) {
        return vec3(0.0);
    } else if (voxel_type == VOXEL_TYPE_DIRT) {
        return voxel_colors.dirt_color;
    } else if (voxel_type == VOXEL_TYPE_SAND) {
        return voxel_colors.sand_color;
    } else if (voxel_type == VOXEL_TYPE_CHERRY_WOOD) {
        return voxel_colors.cherry_wood_color;
    } else if (voxel_type == VOXEL_TYPE_OAK_WOOD) {
        return voxel_colors.oak_wood_color;
    } else if (voxel_type == VOXEL_TYPE_ROCK) {
        return voxel_colors.rock_color;
    }
    return vec3(0.0);
}

float _voxel_hash_variance_lut(uint voxel_type) {
    if (voxel_type == VOXEL_TYPE_DIRT || voxel_type == VOXEL_TYPE_SAND) {
        return 1.0;
    }
    if (voxel_type == VOXEL_TYPE_ROCK) {
        return 0.6;
    }
    return 0.0;
}

vec3 voxel_color_with_hash_srgb(uint voxel_type, uint hash_id) {
    vec3 color = _voxel_color_by_type_srgb(voxel_type);
    vec3 hsv   = rgb_to_hsv(color);

    // 2-bit variation gives 4 deterministic, subtle per-type palette variants.
    float variant = float(hash_id & 0x3u) - 1.5;
    float amount  = voxel_colors.hash_color_variance * _voxel_hash_variance_lut(voxel_type);
    hsv.x         = fract(hsv.x + variant * 0.01 * amount);
    hsv.y         = clamp(hsv.y + variant * 0.03 * amount, 0.0, 1.0);
    hsv.z         = clamp(hsv.z + variant * 0.025 * amount, 0.0, 1.0);

    return hsv_to_rgb(hsv);
}

vec3 voxel_color_by_type_unorm(uint voxel_type) {
    return srgb_to_linear(voxel_color_with_hash_srgb(voxel_type, 0u));
}

vec3 voxel_color_by_type_and_hash_unorm(uint voxel_type, uint hash_id) {
    return srgb_to_linear(voxel_color_with_hash_srgb(voxel_type, hash_id));
}

vec3 apply_terrain_moisture_level(vec3 base_color, uint voxel_type, uint moisture_level) {
    bool can_show_moisture = voxel_type == VOXEL_TYPE_DIRT || voxel_type == VOXEL_TYPE_SAND;
    if (!can_show_moisture || moisture_level == 0u) {
        return base_color;
    }

    uint level = min(moisture_level, VOXEL_MOISTURE_MAX);
    vec3 wet_color;
    if (level == 1u) {
        // Damp: slightly darker, still mostly the dry material color.
        wet_color = mix(base_color * 0.76, vec3(0.060, 0.050, 0.038), 0.16);
    } else if (level == 2u) {
        // Wet: visibly darker and cooler.
        wet_color = mix(base_color * 0.52, vec3(0.030, 0.038, 0.032), 0.28);
    } else {
        // Saturated: darkest, with a subtle cool green/blue cast.
        wet_color = mix(base_color * 0.32, vec3(0.010, 0.024, 0.022), 0.42);
    }
    return clamp(wet_color, vec3(0.0), vec3(1.0));
}

vec3 apply_terrain_fertility_level(vec3 base_color, uint voxel_type, uint fertility_level) {
    bool can_show_fertility = voxel_type == VOXEL_TYPE_DIRT || voxel_type == VOXEL_TYPE_SAND;
    if (!can_show_fertility) {
        return base_color;
    }

    uint level = min(fertility_level, VOXEL_FERTILITY_MAX);
    if (level == 0u) {
        // Barren: slightly pale and desaturated.
        vec3 gray = vec3(dot(base_color, vec3(0.299, 0.587, 0.114)));
        return clamp(mix(base_color, gray, 0.28) * vec3(1.05, 0.98, 0.86), vec3(0.0), vec3(1.0));
    }
    if (level == 1u) {
        // Wild/default soil should read like the normal terrain palette.
        return base_color;
    }

    // Fertilized soil should read as soft compost/light-brown, not bright yellow/orange.
    // Constants are pre-linearized from approximate sRGB #8F7355 and #9C7A58.
    vec3 light_compost_brown = vec3(0.275, 0.171, 0.091);
    vec3 rich_compost_brown = vec3(0.332, 0.195, 0.098);
    vec3 compost_color = level == 2u ? light_compost_brown : rich_compost_brown;
    float tint = level == 2u ? 0.18 : 0.34;
    vec3 material_warmth = level == 2u ? vec3(1.03, 1.00, 0.94) : vec3(1.05, 1.01, 0.91);
    return clamp(mix(base_color * material_warmth, compost_color, tint), vec3(0.0), vec3(1.0));
}

#endif // VOXEL_COLORS_GLSL
