#ifndef FLORA_EXCLUSION_GLSL
#define FLORA_EXCLUSION_GLSL

layout(set = 1, binding = 1) readonly buffer B_GrassExclusionBits {
    uint words[];
}
manual_grass_exclusion_bits;

bool grass_is_excluded(uvec3 local_base, uvec3 chunk_dim) {
    uint linear_idx = local_base.x + chunk_dim.x * (local_base.y + chunk_dim.y * local_base.z);
    uint word = manual_grass_exclusion_bits.words[linear_idx >> 5u];
    return (word & (1u << (linear_idx & 31u))) != 0u;
}

#endif
