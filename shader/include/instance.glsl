#ifndef INSTANCE_GLSL
#define INSTANCE_GLSL

struct Instance {
    uint packed_local_pos;
    // Lower 12 bits: type, upper 20 bits: seed
    uint ty_seed;
    uint growth_start_tick;
};

uint pack_instance_local_pos(uvec3 local_pos) {
    return (local_pos.x & 0xffu) | ((local_pos.y & 0xffu) << 8u) |
           ((local_pos.z & 0xffu) << 16u);
}

uvec3 unpack_instance_local_pos(uint packed_local_pos) {
    return uvec3(packed_local_pos & 0xffu, (packed_local_pos >> 8u) & 0xffu,
                 (packed_local_pos >> 16u) & 0xffu);
}

uvec3 get_instance_world_pos(Instance instance, uvec3 chunk_world_offset) {
    return chunk_world_offset + unpack_instance_local_pos(instance.packed_local_pos);
}

uvec3 get_instance_world_pos(uint packed_local_pos, uvec3 chunk_world_offset) {
    return chunk_world_offset + unpack_instance_local_pos(packed_local_pos);
}

void set_instance_local_pos(inout Instance instance, uvec3 local_pos) {
    instance.packed_local_pos = pack_instance_local_pos(local_pos);
}

const uint INSTANCE_TY_BITS   = 12u;
const uint INSTANCE_SEED_BITS = 20u;
const uint INSTANCE_TY_MASK   = (1u << INSTANCE_TY_BITS) - 1u;
const uint INSTANCE_SEED_MASK = (1u << INSTANCE_SEED_BITS) - 1u;

uint pack_instance_ty_seed(uint ty, uint seed) {
    return (ty & INSTANCE_TY_MASK) | ((seed & INSTANCE_SEED_MASK) << INSTANCE_TY_BITS);
}

uint decode_instance_ty(uint ty_seed) { return ty_seed & INSTANCE_TY_MASK; }

uint decode_instance_seed(uint ty_seed) {
    return (ty_seed >> INSTANCE_TY_BITS) & INSTANCE_SEED_MASK;
}

#endif // INSTANCE_GLSL
