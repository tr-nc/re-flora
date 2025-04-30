#ifndef BVH_GLSL
#define BVH_GLSL

struct BvhNode {
    vec3 aabb_min;
    vec3 aabb_max;
    uint offset;
};

bool is_leaf(BvhNode node) { return (node.offset & 0x80000000u) != 0u; }

// if is leaf: the data is the left pointer to the child bvh node, the right pointer is data + 1
// if not leaf: the data is arbitary data for the leaf
uint fetch_data(BvhNode node) { return node.offset & 0x7FFFFFFFu; }

#endif // BVH_GLSL
