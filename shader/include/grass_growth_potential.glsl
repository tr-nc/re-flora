#ifndef GRASS_GROWTH_POTENTIAL_GLSL
#define GRASS_GROWTH_POTENTIAL_GLSL

#include "./flora_registry.glsl"

// One four-bit ordinary-grass growth-potential level per voxel. This is a target-specific
// competition field: authored flora currently suppress tall and short grass, but never themselves.
// Future general environment inputs (moisture, fertility, sunlight) should be combined separately
// into the final growth factor rather than being mislabeled as grass competition.
layout(set = 1, binding = 1) readonly buffer B_GrassGrowthPotentialLevels {
    uint words[];
}
manual_grass_growth_potential_levels;

const uint GRASS_GROWTH_POTENTIAL_LEVEL_MAX = 15u;
const uvec3 GRASS_GROWTH_POTENTIAL_CHUNK_DIM = uvec3(256u);

uint grass_growth_potential_level(uvec3 local_base, uvec3 chunk_dim) {
    uint linear_idx = local_base.x + chunk_dim.x * (local_base.y + chunk_dim.y * local_base.z);
    uint packed_word = manual_grass_growth_potential_levels.words[linear_idx >> 3u];
    return (packed_word >> ((linear_idx & 7u) * 4u)) & 0x0fu;
}

float grass_growth_potential(uvec3 local_base, uvec3 chunk_dim) {
    return float(grass_growth_potential_level(local_base, chunk_dim)) /
           float(GRASS_GROWTH_POTENTIAL_LEVEL_MAX);
}

float grass_growth_potential(uvec3 local_base) {
    return grass_growth_potential(local_base, GRASS_GROWTH_POTENTIAL_CHUNK_DIM);
}

float flora_competition_growth_factor(uvec3 local_base, uint target_species) {
    bool is_grass = target_species == FLORA_SPECIES_TALL_GRASS ||
                    target_species == FLORA_SPECIES_SHORT_GRASS;
    return is_grass ? grass_growth_potential(local_base) : 1.0;
}

uint grass_growth_progress_limit(uvec3 local_base, uvec3 chunk_dim) {
    uint level = grass_growth_potential_level(local_base, chunk_dim);
    // Existing grass remains visible even at the lowest future potential level.
    return max(1u, (level * INSTANCE_GROWTH_PROGRESS_MATURE + 7u) /
                       GRASS_GROWTH_POTENTIAL_LEVEL_MAX);
}

#endif
