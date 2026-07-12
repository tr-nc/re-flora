#ifndef FLORA_GROWTH_POTENTIAL_GLSL
#define FLORA_GROWTH_POTENTIAL_GLSL

// One four-bit environmental growth-potential level per voxel. This derived field currently
// contains authored-flora proximity influence. Future moisture, fertility, or sunlight systems
// should contribute to this same value rather than adding renderer-facing factors of their own.
layout(set = 1, binding = 1) readonly buffer B_FloraGrowthPotentialLevels {
    uint words[];
}
manual_flora_growth_potential_levels;

const uint FLORA_GROWTH_POTENTIAL_LEVEL_MAX = 15u;
const uvec3 FLORA_GROWTH_POTENTIAL_CHUNK_DIM = uvec3(256u);

uint flora_growth_potential_level(uvec3 local_base, uvec3 chunk_dim) {
    uint linear_idx = local_base.x + chunk_dim.x * (local_base.y + chunk_dim.y * local_base.z);
    uint packed_word = manual_flora_growth_potential_levels.words[linear_idx >> 3u];
    return (packed_word >> ((linear_idx & 7u) * 4u)) & 0x0fu;
}

float flora_growth_potential(uvec3 local_base, uvec3 chunk_dim) {
    return float(flora_growth_potential_level(local_base, chunk_dim)) /
           float(FLORA_GROWTH_POTENTIAL_LEVEL_MAX);
}

float flora_growth_potential(uvec3 local_base) {
    return flora_growth_potential(local_base, FLORA_GROWTH_POTENTIAL_CHUNK_DIM);
}

uint flora_growth_progress_limit(uvec3 local_base, uvec3 chunk_dim) {
    uint level = flora_growth_potential_level(local_base, chunk_dim);
    // Existing grass remains visible even at the lowest future potential level.
    return max(1u, (level * INSTANCE_GROWTH_PROGRESS_MATURE + 7u) /
                       FLORA_GROWTH_POTENTIAL_LEVEL_MAX);
}

#endif
