#ifndef TERRAIN_SMOOTH_MBO_COMMON_GLSL
#define TERRAIN_SMOOTH_MBO_COMMON_GLSL

#include "../../include/voxel_data.glsl"
#include "../../include/voxel_types.glsl"

const float TERRAIN_SMOOTH_MBO_DEAD_SCORE_SCALE = 1.0 / 256.0;

uint local_index(uvec3 local) {
    return (local.z * terrain_smooth_mbo_info.dim.y + local.y) * terrain_smooth_mbo_info.dim.x + local.x;
}

bool in_local_bounds(ivec3 local) {
    return all(greaterThanEqual(local, ivec3(0))) &&
           all(lessThan(local, ivec3(terrain_smooth_mbo_info.dim.xyz)));
}

ivec3 atlas_pos(uvec3 local) { return ivec3(terrain_smooth_mbo_info.offset.xyz + local); }

uint load_voxel_type(uvec3 local) { return voxel_type_from_atlas_data(imageLoad(chunk_atlas, atlas_pos(local)).r); }

bool is_terrain_voxel(uint voxel_type) {
    return voxel_type == VOXEL_TYPE_DIRT || voxel_type == VOXEL_TYPE_SAND ||
           voxel_type == VOXEL_TYPE_ROCK;
}

bool is_mutable_voxel(uint voxel_type) {
    return voxel_type == VOXEL_TYPE_EMPTY || is_terrain_voxel(voxel_type);
}

float original_density(uint voxel_type) { return is_terrain_voxel(voxel_type) ? 1.0 : 0.0; }

vec3 voxel_center(uvec3 local) {
    return vec3(terrain_smooth_mbo_info.offset.xyz + local) + vec3(0.5);
}

float brush_falloff(uvec3 local) {
    float dist = length(voxel_center(local) - terrain_smooth_mbo_info.center_radius.xyz);
    if (dist > terrain_smooth_mbo_info.center_radius.w) {
        return 0.0;
    }
    float t = clamp(dist / max(terrain_smooth_mbo_info.center_radius.w, 0.0001), 0.0, 1.0);
    return 1.0 - smoothstep(0.0, 1.0, t);
}

bool is_candidate_voxel(uvec3 local, uint voxel_type) {
    return is_mutable_voxel(voxel_type) && brush_falloff(local) > 0.0;
}

uint score_bin(float score) {
    float clamped_score = clamp(score, 0.0, 1.0);
    uint bin_count = max(terrain_smooth_mbo_info.dim.w, 1u);
    return min(uint(floor(clamped_score * float(bin_count - 1u) + 0.5)), bin_count - 1u);
}

uint smooth_hash(uvec3 pos) {
    uint x = pos.x * 0x9E3779B9u ^ pos.y * 0x85EBCA6Bu ^ pos.z * 0xC2B2AE35u;
    x ^= x >> 16u;
    x *= 0x7FEB352Du;
    x ^= x >> 15u;
    x *= 0x846CA68Bu;
    return x ^ (x >> 16u);
}

uint choose_fill_voxel_type(uvec3 local) {
    uint dirt_count = 0u;
    uint sand_count = 0u;
    uint rock_count = 0u;

    const ivec3 offsets[6] = {
        ivec3(1, 0, 0),  ivec3(-1, 0, 0), ivec3(0, 1, 0),
        ivec3(0, -1, 0), ivec3(0, 0, 1),  ivec3(0, 0, -1),
    };
    for (int i = 0; i < 6; ++i) {
        ivec3 next = ivec3(local) + offsets[i];
        if (!in_local_bounds(next)) {
            continue;
        }
        uint voxel_type = load_voxel_type(uvec3(next));
        if (voxel_type == VOXEL_TYPE_DIRT) {
            dirt_count += 1u;
        } else if (voxel_type == VOXEL_TYPE_SAND) {
            sand_count += 1u;
        } else if (voxel_type == VOXEL_TYPE_ROCK) {
            rock_count += 1u;
        }
    }

    if (sand_count > dirt_count && sand_count >= rock_count) {
        return VOXEL_TYPE_SAND;
    }
    if (rock_count > dirt_count && rock_count > sand_count) {
        return VOXEL_TYPE_ROCK;
    }
    return VOXEL_TYPE_DIRT;
}

#endif // TERRAIN_SMOOTH_MBO_COMMON_GLSL
