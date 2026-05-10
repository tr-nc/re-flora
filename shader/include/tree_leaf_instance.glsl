#ifndef TREE_LEAF_INSTANCE_GLSL
#define TREE_LEAF_INSTANCE_GLSL

struct TreeLeafInstance {
    uint packed_local_pos;
    uint packed_orientation;
};

const uint TREE_LEAF_LOCAL_POS_MASK = 0x3ffu;
const uint TREE_LEAF_GROWTH_PROGRESS = 0xffu;

uvec3 unpack_tree_leaf_local_pos(uint packed_local_pos) {
    return uvec3(packed_local_pos & TREE_LEAF_LOCAL_POS_MASK,
                 (packed_local_pos >> 10u) & TREE_LEAF_LOCAL_POS_MASK,
                 (packed_local_pos >> 20u) & TREE_LEAF_LOCAL_POS_MASK);
}

uvec3 get_tree_leaf_world_pos(uint packed_local_pos, uvec3 chunk_world_offset) {
    return chunk_world_offset + unpack_tree_leaf_local_pos(packed_local_pos);
}

#endif // TREE_LEAF_INSTANCE_GLSL
