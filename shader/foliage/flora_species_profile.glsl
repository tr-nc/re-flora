#ifndef FLORA_SPECIES_PROFILE_GLSL
#define FLORA_SPECIES_PROFILE_GLSL

// Species-level voxel semantics live here instead of in per-vertex attributes.
// The shader can derive shared behavior from the species id and authored local voxel position
// without increasing instance/vertex memory.

const uint FLORA_WIND_MOTION_WHOLE_INSTANCE = 0u;
const uint FLORA_WIND_MOTION_GROUPED_STEMS = 1u;
const uint FLORA_WIND_MOTION_LOCAL_VOXELS = 2u;

uint flora_species_wind_motion_topology(uint instance_ty) {
    if (instance_ty == FLORA_SPECIES_KOCHIA) {
        return FLORA_WIND_MOTION_GROUPED_STEMS;
    }
    if (instance_ty == FLORA_SPECIES_TALL_GRASS ||
        instance_ty == FLORA_SPECIES_SHORT_GRASS || instance_ty == FLORA_SPECIES_APPLE) {
        return FLORA_WIND_MOTION_WHOLE_INSTANCE;
    }
    return FLORA_WIND_MOTION_LOCAL_VOXELS;
}

uint flora_grouped_stem_wind_seed(uint instance_seed, uint animation_group) {
    // Cycling consecutive stems through the global wind buckets gives every clump a balanced
    // update cadence. The instance seed rotates that cadence between neighboring plants.
    uint stem_offset = animation_group > 0u ? animation_group - 1u : 0u;
    return instance_seed + stem_offset;
}

float flora_species_voxel_wind_affect_multiplier(uint instance_ty, ivec3 vox_local_pos,
                                                  float wind_gradient) {
    if (instance_ty == FLORA_SPECIES_KOCHIA) {
        return max(gui_input.kochia_body_wind_response, 0.0);
    }
    return 1.0;
}

#endif // FLORA_SPECIES_PROFILE_GLSL
