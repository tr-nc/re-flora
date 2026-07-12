#ifndef FLORA_ENVIRONMENT_GLSL
#define FLORA_ENVIRONMENT_GLSL

#include "./flora_registry.glsl"
#include "./voxel_data.glsl"

// Surface-flora instances store the supporting terrain voxel as their local base position.
// Reading that exact atlas texel avoids guessing from the visible stem, which starts one voxel up.
layout(set = 0, binding = 15, r8ui) readonly uniform uimage3D chunk_atlas;

float flora_moisture_growth_factor(uvec3 local_base, uvec3 chunk_world_offset,
                                   uint target_species) {
    if (target_species >= FLORA_SPECIES_COUNT) {
        return 1.0;
    }

    uvec3 atlas_pos = chunk_world_offset + local_base;
    uvec3 atlas_dim = uvec3(imageSize(chunk_atlas));
    if (any(greaterThanEqual(atlas_pos, atlas_dim))) {
        return 1.0;
    }

    uint atlas_data = imageLoad(chunk_atlas, ivec3(atlas_pos)).r;
    uint moisture_level = min(voxel_moisture_from_atlas_data(atlas_data), VOXEL_MOISTURE_MAX);
    return flora_growth_info.moisture_growth_factors[target_species][moisture_level];
}

float flora_environment_growth_factor(uvec3 local_base, uvec3 chunk_world_offset,
                                      uint target_species) {
    // Future fertility and sunlight factors belong here, independently of species competition.
    return flora_moisture_growth_factor(local_base, chunk_world_offset, target_species);
}

#endif
