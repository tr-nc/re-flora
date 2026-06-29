#ifndef TREE_LEAF_INSTANCE_GLSL
#define TREE_LEAF_INSTANCE_GLSL

struct TreeLeafInstance {
    uint packed_local_pos;
    uint packed_leaf_local_pos;
};

const uint TREE_LEAF_LOCAL_POS_MASK = 0x3ffu;
const uint TREE_LEAF_VOXEL_LOCAL_POS_MASK = 0x3ffu;
const int TREE_LEAF_VOXEL_LOCAL_POS_BIAS = 512;
const uint TREE_LEAF_GROWTH_PROGRESS = 0xffu;

uvec3 unpack_tree_leaf_local_pos(uint packed_local_pos) {
    return uvec3(packed_local_pos & TREE_LEAF_LOCAL_POS_MASK,
                 (packed_local_pos >> 10u) & TREE_LEAF_LOCAL_POS_MASK,
                 (packed_local_pos >> 20u) & TREE_LEAF_LOCAL_POS_MASK);
}

int unpack_tree_leaf_signed_10(uint packed_value) {
    return int(packed_value & TREE_LEAF_VOXEL_LOCAL_POS_MASK) - TREE_LEAF_VOXEL_LOCAL_POS_BIAS;
}

ivec3 unpack_tree_leaf_voxel_local_pos(uint packed_leaf_local_pos) {
    return ivec3(unpack_tree_leaf_signed_10(packed_leaf_local_pos),
                 unpack_tree_leaf_signed_10(packed_leaf_local_pos >> 10u),
                 unpack_tree_leaf_signed_10(packed_leaf_local_pos >> 20u));
}

uvec3 get_tree_leaf_world_pos(uint packed_local_pos, uvec3 chunk_world_offset) {
    return chunk_world_offset + unpack_tree_leaf_local_pos(packed_local_pos);
}

#endif // TREE_LEAF_INSTANCE_GLSL
