#ifndef VOXEL_DATA_GLSL
#define VOXEL_DATA_GLSL

const uint VOXEL_TYPE_MASK          = 0x0Fu;
const uint VOXEL_ATLAS_STATE_MASK   = 0xF0u;
// Moisture intentionally uses only two bits (4..5): 0=dry, 1..3=wetter.
// Bits 6..7 stay reserved for future soil state such as tilled/fertility flags.
const uint VOXEL_MOISTURE_MASK      = 0x30u;
const uint VOXEL_MOISTURE_SHIFT     = 4u;
const uint VOXEL_MOISTURE_MAX       = 3u;
const uint VOXEL_NORMAL_BITS_MASK   = 0x1FFFFFu;
const uint VOXEL_NORMAL_VALID_MASK  = 1u << 29u;
const uint VOXEL_HASH_MASK          = 0x3u;
const uint VOXEL_HASH_SHIFT         = 30u;

uint voxel_type_from_data(uint voxel_data) { return voxel_data & VOXEL_TYPE_MASK; }

uint voxel_type_from_atlas_data(uint voxel_data) { return voxel_data & VOXEL_TYPE_MASK; }

uint voxel_moisture_from_atlas_data(uint voxel_data) {
    return (voxel_data & VOXEL_MOISTURE_MASK) >> VOXEL_MOISTURE_SHIFT;
}

uint pack_voxel_atlas_data(uint voxel_type, uint moisture) {
    return (voxel_type & VOXEL_TYPE_MASK) |
           ((moisture & VOXEL_MOISTURE_MAX) << VOXEL_MOISTURE_SHIFT);
}

uint pack_voxel_atlas_type_clear_state(uint voxel_type) {
    return pack_voxel_atlas_data(voxel_type, 0u);
}

uint pack_voxel_atlas_moisture_preserve_state(uint old_voxel_data, uint moisture) {
    return (old_voxel_data & ~VOXEL_MOISTURE_MASK) |
           ((moisture & VOXEL_MOISTURE_MAX) << VOXEL_MOISTURE_SHIFT);
}

uint pack_voxel_atlas_type_preserve_state(uint old_voxel_data, uint voxel_type) {
    return (old_voxel_data & VOXEL_ATLAS_STATE_MASK) | (voxel_type & VOXEL_TYPE_MASK);
}

uint voxel_normal_bits_from_data(uint voxel_data) {
    return (voxel_data >> 8u) & VOXEL_NORMAL_BITS_MASK;
}

bool voxel_normal_valid_from_data(uint voxel_data) {
    return (voxel_data & VOXEL_NORMAL_VALID_MASK) != 0u;
}

uint voxel_hash_from_data(uint voxel_data) {
    return (voxel_data >> VOXEL_HASH_SHIFT) & VOXEL_HASH_MASK;
}

uint pack_voxel_surface_data(uint voxel_type, uint normal_bits, bool is_normal_valid,
                             uint hash_id) {
    uint voxel_data = 0u;
    voxel_data |= voxel_type & VOXEL_TYPE_MASK;
    voxel_data |= (normal_bits & VOXEL_NORMAL_BITS_MASK) << 8u;
    voxel_data |= (is_normal_valid ? 1u : 0u) << 29u;
    voxel_data |= (hash_id & VOXEL_HASH_MASK) << VOXEL_HASH_SHIFT;
    return voxel_data;
}

uint voxel_hash_from_world_pos(ivec3 world_pos, uint voxel_type) {
    uint h = uint(world_pos.x) * 73856093u;
    h ^= uint(world_pos.y) * 19349663u;
    h ^= uint(world_pos.z) * 83492791u;
    h ^= voxel_type * 2654435761u;
    h ^= h >> 16u;
    h *= 2246822519u;
    h ^= h >> 13u;
    return h & VOXEL_HASH_MASK;
}

#endif // VOXEL_DATA_GLSL
