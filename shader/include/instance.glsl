#ifndef INSTANCE_GLSL
#define INSTANCE_GLSL

struct Instance {
    uint packed_local_pos;
    // Lower 12 bits: type, upper 20 bits: seed
    uint ty_seed;
};

const uint INSTANCE_GROWTH_PROGRESS_MATURE = 0xffu;
const uint INSTANCE_GROWTH_PROGRESS_REBIRTH = 0x0au;

uint pack_instance_local_pos_growth(uvec3 local_pos, uint growth_progress) {
    return (local_pos.x & 0xffu) | ((local_pos.y & 0xffu) << 8u) |
           ((local_pos.z & 0xffu) << 16u) | ((growth_progress & 0xffu) << 24u);
}

uvec3 unpack_instance_local_pos(uint packed_local_pos) {
    return uvec3(packed_local_pos & 0xffu, (packed_local_pos >> 8u) & 0xffu,
                 (packed_local_pos >> 16u) & 0xffu);
}

uint unpack_instance_growth_progress(uint packed_local_pos) {
    return (packed_local_pos >> 24u) & 0xffu;
}

uint set_instance_growth_progress(uint packed_local_pos, uint growth_progress) {
    return (packed_local_pos & 0x00ffffffu) | ((growth_progress & 0xffu) << 24u);
}

uvec3 get_instance_world_pos(Instance instance, uvec3 chunk_world_offset) {
    return chunk_world_offset + unpack_instance_local_pos(instance.packed_local_pos);
}

uvec3 get_instance_world_pos(uint packed_local_pos, uvec3 chunk_world_offset) {
    return chunk_world_offset + unpack_instance_local_pos(packed_local_pos);
}

void set_instance_local_pos_growth(inout Instance instance, uvec3 local_pos, uint growth_progress) {
    instance.packed_local_pos = pack_instance_local_pos_growth(local_pos, growth_progress);
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
