#ifndef FLORA_SPECIES_PROFILE_GLSL
#define FLORA_SPECIES_PROFILE_GLSL

// Species-level voxel semantics live here instead of in per-vertex attributes.
// The shader can derive shared behavior from the species id and authored local voxel position
// without increasing instance/vertex memory.

float flora_species_voxel_wind_affect_multiplier(uint instance_ty, ivec3 vox_local_pos,
                                                  float wind_gradient) {
    return 1.0;
}

#endif // FLORA_SPECIES_PROFILE_GLSL
