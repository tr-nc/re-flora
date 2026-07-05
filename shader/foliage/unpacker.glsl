#ifndef UNPACKER_GLSL
#define UNPACKER_GLSL

const uint BIT_PER_POS    = 7u;
const uint BIT_PER_OFFSET = 1u;

const uint POS_BITS    = BIT_PER_POS * 3u;
const uint OFFSET_BITS = BIT_PER_OFFSET * 3u;

const uint POS_MASK    = (1u << BIT_PER_POS) - 1u;
const uint OFFSET_MASK = (1u << BIT_PER_OFFSET) - 1u;

void unpack_vertex_data(out ivec3 o_vox_local_pos, out uvec3 o_vert_offset_in_vox,
                        uint packed_data) {
    uint pos_x = packed_data & POS_MASK;
    uint pos_y = (packed_data >> BIT_PER_POS) & POS_MASK;
    uint pos_z = (packed_data >> (BIT_PER_POS * 2u)) & POS_MASK;

    const int POS_OFFSET = 1 << (BIT_PER_POS - 1u);
    o_vox_local_pos      = ivec3(pos_x, pos_y, pos_z) - POS_OFFSET;

    uint offset_packed = (packed_data >> POS_BITS) & ((1u << OFFSET_BITS) - 1u);
    o_vert_offset_in_vox =
        uvec3(offset_packed & OFFSET_MASK, (offset_packed >> BIT_PER_OFFSET) & OFFSET_MASK,
              (offset_packed >> (BIT_PER_OFFSET * 2u)) & OFFSET_MASK);
}

#endif // UNPACKER_GLSL
