#ifndef UNPACKER_GLSL
#define UNPACKER_GLSL

// Convert vertex offset index to actual 3D offset
uvec3 decode_vertex_offset(uint vertex_offset) {
    // The vertex offset is encoded as: x | (y << 1) | (z << 2)
    // So we need to extract each bit
    uint x = vertex_offset & 1u;
    uint y = (vertex_offset >> 1) & 1u;
    uint z = (vertex_offset >> 2) & 1u;
    return uvec3(x, y, z);
}

void unpack_vertex_data(out ivec3 o_vox_local_pos, out uvec3 o_vert_offset_in_vox,
                        out float o_gradient, uint packed_data) {
    o_vox_local_pos =
        ivec3(packed_data & 0xFF, (packed_data >> 8) & 0xFF, (packed_data >> 16) & 0xFF);
    o_vox_local_pos -= ivec3(128);
    o_vert_offset_in_vox = decode_vertex_offset((packed_data >> 24) & 0x7u);
    o_gradient           = float((packed_data >> 27) & 0x1F) / 31.0;
}

#endif // UNPACKER_GLSL
