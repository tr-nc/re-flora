#ifndef BVH_GLSL
#define BVH_GLSL

struct BvhNode {
    vec3 aabb_min;
    vec3 aabb_max;
    /// If the leftmost bit is 0 ⇒ internal, `offset` = left_child_index
    /// If the leftmost bit is 1 ⇒ leaf,     `offset` = (1<<31) | primitive_index
    uint offset;
};

bool is_leaf(BvhNode node) { return (node.offset & 0x80000000u) != 0u; }

#endif // BVH_GLSL
