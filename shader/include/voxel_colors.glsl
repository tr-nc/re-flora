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

float terrain_fertilizer_granule_noise(vec3 center_pos, uint seed) {
    ivec3 p = ivec3(floor(center_pos * 256.0));
    uint h = seed ^ 0x9E3779B9u;
    h ^= uint(p.x) * 0x85EBCA6Bu;
    h ^= uint(p.y) * 0xC2B2AE35u;
    h ^= uint(p.z) * 0x27D4EB2Fu;
    h ^= h >> 16u;
    h *= 0x7FEB352Du;
    h ^= h >> 15u;
    h *= 0x846CA68Bu;
    h ^= h >> 16u;
    return float(h & 0x00FFFFFFu) * (1.0 / 16777216.0);
}

vec3 apply_terrain_fertility_level(vec3 base_color, uint voxel_type, uint fertility_level,
                                   vec3 center_pos) {
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

    // Fertilized soil should read as solid slow-release compost granules on soil,
    // not a bright liquid/yellow wash. The broad tint stays subtle; discrete
    // high-threshold speckles carry most of the fresh surface-fertilizer read.
    // Constants are linearized from soft brown sRGB compost/pellet colors.
    vec3 light_compost_brown = vec3(0.275, 0.171, 0.091); // #8F7355
    vec3 rich_compost_brown = vec3(0.332, 0.195, 0.098);  // #9C7A58
    vec3 pale_pellet = vec3(0.456, 0.332, 0.205);        // #B99C7D
    vec3 dark_pellet = vec3(0.172, 0.100, 0.050);        // #735939

    vec3 compost_color = level == 2u ? light_compost_brown : rich_compost_brown;
    float tint = level == 2u ? 0.10 : 0.18;
    vec3 material_warmth = level == 2u ? vec3(1.02, 1.00, 0.96) : vec3(1.04, 1.01, 0.94);
    vec3 fertile_color = mix(base_color * material_warmth, compost_color, tint);

    float fine_noise = terrain_fertilizer_granule_noise(center_pos, 0x6D2B79F5u);
    float coarse_noise = terrain_fertilizer_granule_noise(center_pos * 0.31 + vec3(11.0), 0xA511E9B3u);
    float granule_threshold = level == 2u ? 0.80 : 0.62;
    float granule = smoothstep(granule_threshold, 0.98, mix(fine_noise, coarse_noise, 0.28));
    vec3 granule_color = mix(dark_pellet, pale_pellet, terrain_fertilizer_granule_noise(center_pos, 0x68BC21EBu));
    float granule_opacity = level == 2u ? 0.34 : 0.50;
    return clamp(mix(fertile_color, granule_color, granule * granule_opacity), vec3(0.0), vec3(1.0));
}

#endif // VOXEL_COLORS_GLSL
