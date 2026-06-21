#ifndef FLORA_PLACEMENT_GLSL
#define FLORA_PLACEMENT_GLSL

#include "./core/gradient_noise.glsl"
#include "./flora_registry.glsl"

// Legacy placement hash used by the flora shaders before paint modes were added.
// Keep this exact function for stable flora density/species seeds across rebuilds.
float flora_legacy_hash(vec2 p) {
    vec3 p3 = fract(vec3(p.xyx) * .1031);
    p3 += dot(p3, p3.yzx + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

const float FLORA_LAVENDER_SPARSE_THRESHOLD        = 0.10;
const float FLORA_EMBER_BLOOM_SPARSE_THRESHOLD     = 0.01;
const float FLORA_GRASS_DECREASED_SPARSE_THRESHOLD = 0.45;

void calculate_flora_density_placement(ivec3 world_pos, out bool o_allow_any_flora,
                                       out bool o_allow_tall_grass,
                                       out bool o_allow_short_grass) {
    float rng_any_flora   = flora_legacy_hash(vec2(world_pos.x, world_pos.z));
    float rng_tall_grass  = flora_legacy_hash(vec2(world_pos.x + 17.0, world_pos.z - 29.0));
    float rng_short_grass = flora_legacy_hash(vec2(world_pos.x - 53.0, world_pos.z + 91.0));

    float density_noise = cnoise_seeded(vec2(float(world_pos.x), float(world_pos.z)) * 5.0, 42u);

    float placement_threshold = (density_noise + 1.0) * 0.5;
    placement_threshold       = clamp(0.08 + placement_threshold * 0.22, 0.0, 1.0);

    float grass_placement_threshold = clamp(placement_threshold * 2.25, 0.0, 1.0);
    float tall_grass_threshold      = clamp(grass_placement_threshold * 0.30, 0.0, 1.0);
    float short_grass_threshold     = clamp(grass_placement_threshold * 0.8, 0.0, 1.0);

    o_allow_any_flora   = rng_any_flora < placement_threshold;
    o_allow_tall_grass  = rng_tall_grass < tall_grass_threshold;
    o_allow_short_grass = rng_short_grass < short_grass_threshold;
}

bool flora_species_uses_sparse_paint_density(uint species_idx) {
    return species_idx == FLORA_SPECIES_LAVENDER || species_idx == FLORA_SPECIES_EMBER_BLOOM;
}

float flora_sparse_species_rng(ivec3 world_pos) {
    return flora_legacy_hash(vec2(world_pos.x + 42.0, world_pos.z - 15.0));
}

bool flora_sparse_species_mask_allows(uint species_idx, ivec3 world_pos) {
    float species_rng = flora_sparse_species_rng(world_pos);

    if (species_idx == FLORA_SPECIES_LAVENDER) {
        return species_rng < FLORA_LAVENDER_SPARSE_THRESHOLD;
    }

    if (species_idx == FLORA_SPECIES_EMBER_BLOOM) {
        return species_rng < FLORA_EMBER_BLOOM_SPARSE_THRESHOLD;
    }

    return true;
}

bool flora_paint_selection_uses_sparse_density(uint paint_selection) {
    return paint_selection < FLORA_SPECIES_COUNT &&
           flora_species_uses_sparse_paint_density(paint_selection);
}

bool flora_sparse_paint_selection_allows_with_density(uint paint_selection, ivec3 world_pos,
                                                      bool allow_any_flora) {
    if (!flora_paint_selection_uses_sparse_density(paint_selection)) {
        return true;
    }

    return allow_any_flora && flora_sparse_species_mask_allows(paint_selection, world_pos);
}

bool flora_sparse_paint_selection_allows(uint paint_selection, ivec3 world_pos) {
    bool allow_any_flora, allow_tall_grass, allow_short_grass;
    calculate_flora_density_placement(world_pos, allow_any_flora, allow_tall_grass,
                                      allow_short_grass);
    return flora_sparse_paint_selection_allows_with_density(paint_selection, world_pos,
                                                            allow_any_flora);
}

#endif // FLORA_PLACEMENT_GLSL
