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

// Paint placement uses a stratified mask rather than the natural random density
// noise. Sparse flowers still land on deterministic world-space seeds, but every
// screen-sized brush area gets a more even spread of candidate points.
const uint FLORA_LAVENDER_PAINT_SPARSE_CELL_SIZE    = 5u;
const uint FLORA_EMBER_BLOOM_PAINT_SPARSE_CELL_SIZE = 10u;

bool flora_stratified_sparse_paint_mask_allows(uint cell_size, vec2 seed_offset,
                                               ivec3 world_pos) {
    uint safe_cell_size = max(cell_size, 1u);
    int cell_size_i     = int(safe_cell_size);
    ivec2 world_xz      = ivec2(world_pos.x, world_pos.z);
    ivec2 cell = ivec2(floor(vec2(float(world_xz.x), float(world_xz.y)) / float(cell_size_i)));
    ivec2 local = world_xz - cell * cell_size_i;

    vec2 cell_seed = vec2(float(cell.x), float(cell.y)) + seed_offset;
    uint anchor_x = min(uint(floor(flora_legacy_hash(cell_seed) * float(safe_cell_size))),
                        safe_cell_size - 1u);
    uint anchor_z = min(uint(floor(flora_legacy_hash(cell_seed + vec2(37.0, 71.0)) *
                                   float(safe_cell_size))),
                        safe_cell_size - 1u);

    return uint(local.x) == anchor_x && uint(local.y) == anchor_z;
}

bool flora_paint_selection_uses_sparse_density(uint paint_selection) {
    return paint_selection < FLORA_SPECIES_COUNT &&
           flora_species_uses_sparse_paint_density(paint_selection);
}

bool flora_sparse_paint_selection_allows(uint paint_selection, ivec3 world_pos) {
    if (paint_selection == FLORA_SPECIES_LAVENDER) {
        return flora_stratified_sparse_paint_mask_allows(
            FLORA_LAVENDER_PAINT_SPARSE_CELL_SIZE, vec2(42.0, -15.0), world_pos);
    }

    if (paint_selection == FLORA_SPECIES_EMBER_BLOOM) {
        return flora_stratified_sparse_paint_mask_allows(
            FLORA_EMBER_BLOOM_PAINT_SPARSE_CELL_SIZE, vec2(-53.0, 91.0), world_pos);
    }

    return true;
}

#endif // FLORA_PLACEMENT_GLSL
