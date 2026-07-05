#ifndef FLORA_SPECIES_PROFILE_GLSL
#define FLORA_SPECIES_PROFILE_GLSL

// Species-level voxel semantics live here instead of in per-vertex attributes.
// The shader can derive shared behavior from the species id and authored local voxel position,
// so adding crops with roots, leaves, blooms, or fruits does not increase instance/vertex memory.

float flora_species_voxel_wind_affect_multiplier(uint instance_ty, ivec3 vox_local_pos,
                                                  float wind_gradient) {
    if (instance_ty == FLORA_SPECIES_CARROT) {
        // Carrot is a root crop: the buried root and small exposed orange shoulder are rigid,
        // while the leafy top starts to catch wind above the soil line.
        return smoothstep(2.0, 5.0, float(vox_local_pos.y));
    }

    if (instance_ty == FLORA_SPECIES_TOMATO) {
        // Keep the first tomato pass fully authored and repeatable: every plant shares the same
        // branch silhouette, with no per-instance rest-bend or wind-seed deformation.
        return 0.0;
    }

    return 1.0;
}

#endif // FLORA_SPECIES_PROFILE_GLSL
