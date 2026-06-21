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

// Paint placement uses a stratified, layered mask rather than the natural random density noise.
// The current paint dab chooses one or more layers inside each cell; repeated dabs visit new
// layers until the per-species max density is reached.
// paint_config: x=dab_serial, y=cell_size_voxels, z=max_plants_per_cell,
//               w=plants_per_cell_per_dab.
const uint FLORA_PAINT_MAX_PLANTS_PER_CELL_PER_DAB = 16u;

uint flora_uint_gcd(uint a, uint b) {
    for (uint i = 0u; i < 32u && b != 0u; ++i) {
        uint r = a % b;
        a      = b;
        b      = r;
    }
    return a;
}

uint flora_coprime_step(uint raw_step, uint modulus) {
    if (modulus <= 1u) {
        return 0u;
    }

    uint step = raw_step % modulus;
    if (step == 0u) {
        step = 1u;
    }

    for (uint attempt = 0u; attempt < 64u; ++attempt) {
        if (flora_uint_gcd(step, modulus) == 1u) {
            return step;
        }
        step += 1u;
        if (step >= modulus) {
            step = 1u;
        }
    }

    return 1u;
}

uvec2 flora_layered_sparse_paint_anchor(uint cell_size, ivec2 cell, uint layer,
                                        vec2 seed_offset) {
    uint safe_cell_size = max(cell_size, 1u);
    uint cell_area      = safe_cell_size * safe_cell_size;
    if (cell_area <= 1u) {
        return uvec2(0u, 0u);
    }

    vec2 cell_seed = vec2(float(cell.x), float(cell.y)) + seed_offset;
    // Layer 0 intentionally matches the previous single-anchor paint mask so existing saved
    // flowers stay on-grid under the new accumulative paint model.
    uint base_x = min(uint(floor(flora_legacy_hash(cell_seed) * float(safe_cell_size))),
                      safe_cell_size - 1u);
    uint base_z = min(uint(floor(flora_legacy_hash(cell_seed + vec2(37.0, 71.0)) *
                                 float(safe_cell_size))),
                      safe_cell_size - 1u);
    uint base_slot = base_z * safe_cell_size + base_x;
    uint raw_step = 1u + min(uint(floor(flora_legacy_hash(cell_seed + vec2(113.0, 197.0)) *
                                        float(cell_area - 1u))),
                             cell_area - 2u);
    uint step = flora_coprime_step(raw_step, cell_area);
    uint slot = (base_slot + (layer % cell_area) * step) % cell_area;

    return uvec2(slot % safe_cell_size, slot / safe_cell_size);
}

bool flora_layered_sparse_paint_mask_allows(uvec4 paint_config, vec2 seed_offset,
                                            ivec3 world_pos) {
    uint cell_size           = max(paint_config.y, 1u);
    uint cell_area           = cell_size * cell_size;
    uint max_plants_per_cell = min(paint_config.z, cell_area);
    uint released_per_dab = min(min(paint_config.w, max_plants_per_cell),
                                FLORA_PAINT_MAX_PLANTS_PER_CELL_PER_DAB);

    if (max_plants_per_cell == 0u || released_per_dab == 0u) {
        return false;
    }

    int cell_size_i = int(cell_size);
    ivec2 world_xz  = ivec2(world_pos.x, world_pos.z);
    ivec2 cell = ivec2(floor(vec2(float(world_xz.x), float(world_xz.y)) / float(cell_size_i)));
    ivec2 local = world_xz - cell * cell_size_i;
    uvec2 local_u = uvec2(local);

    uint first_layer = ((paint_config.x % max_plants_per_cell) * released_per_dab) %
                       max_plants_per_cell;
    for (uint i = 0u; i < FLORA_PAINT_MAX_PLANTS_PER_CELL_PER_DAB; ++i) {
        if (i >= released_per_dab) {
            break;
        }

        uint layer  = (first_layer + i) % max_plants_per_cell;
        uvec2 anchor = flora_layered_sparse_paint_anchor(cell_size, cell, layer, seed_offset);
        if (all(equal(local_u, anchor))) {
            return true;
        }
    }

    return false;
}

bool flora_sparse_paint_selection_allows(uint paint_selection, ivec3 world_pos,
                                         uvec4 paint_config) {
    if (paint_selection == FLORA_SPECIES_LAVENDER) {
        return flora_layered_sparse_paint_mask_allows(
            paint_config, vec2(42.0, -15.0), world_pos);
    }

    if (paint_selection == FLORA_SPECIES_EMBER_BLOOM) {
        return flora_layered_sparse_paint_mask_allows(
            paint_config, vec2(-53.0, 91.0), world_pos);
    }

    return true;
}

#endif // FLORA_PLACEMENT_GLSL
