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

float terrain_moisture_at(vec3 center_pos) {
    int patch_count = int(clamp(voxel_colors.moisture_params.x, 0.0,
                                float(TERRAIN_MOISTURE_PATCH_CAPACITY)));
    float wetness = 0.0;
    for (int i = 0; i < TERRAIN_MOISTURE_PATCH_CAPACITY; ++i) {
        if (i >= patch_count) {
            break;
        }

        vec4 wet_patch = voxel_colors.moisture_patches[i];
        float radius   = max(wet_patch.z, 0.0001);
        float dist     = length(center_pos.xz - wet_patch.xy);
        float edge     = max(radius * 0.45, 0.004);
        float mask     = 1.0 - smoothstep(radius - edge, radius, dist);
        wetness        = max(wetness, mask * wet_patch.w);
    }
    return clamp(wetness * voxel_colors.moisture_params.y, 0.0, 1.0);
}

vec3 apply_terrain_moisture_wetness(vec3 base_color, uint voxel_type, float wetness) {
    bool can_show_moisture = voxel_type == VOXEL_TYPE_DIRT || voxel_type == VOXEL_TYPE_SAND;
    if (!can_show_moisture || wetness <= 0.0) {
        return base_color;
    }

    wetness = pow(clamp(wetness, 0.0, 1.0), 0.58);
    vec3 cool_wet_color = mix(base_color * 0.26, vec3(0.012, 0.028, 0.023), 0.42);
    return mix(base_color, cool_wet_color, clamp(wetness * 1.18, 0.0, 1.0));
}

vec3 apply_terrain_moisture(vec3 base_color, uint voxel_type, vec3 center_pos) {
    return apply_terrain_moisture_wetness(base_color, voxel_type, terrain_moisture_at(center_pos));
}

#endif // VOXEL_COLORS_GLSL
