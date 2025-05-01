#ifndef BVH_SVO_MARCHING_GLSL
#define BVH_SVO_MARCHING_GLSL

#include "./bvh.glsl"
#include "./core/aabb.glsl"
#include "./svo_marching.glsl"

struct BvhSvoMarchingResult {
    bool is_hit;
    uint total_iter; // all iterations for all svos
    SvoMarchingResult hit_svo_result;
};

struct StackInfo {
    uint node_index;
    uint depth;
};

// -----------------------------------------------------------------------------
// BVH traversal that returns the NEAREST real surface hit.
// -----------------------------------------------------------------------------
BvhSvoMarchingResult traverse_bvh(vec3 o, vec3 d, vec3 inv_d) {
    BvhSvoMarchingResult res;
    res.is_hit     = false;
    res.total_iter = 0u; // keep a running total for statistics only

    float best_hit_depth = 1e30; // +INF : nothing hit yet

    // Small explicit stack ----------------------------------------------------
    const uint STACK_SIZE = 100u;
    StackInfo stack[STACK_SIZE];
    uint sp = 0u;

    stack[sp++] = StackInfo(0u, 0u); // push root

    while (sp > 0u) {
        // pop
        StackInfo si = stack[--sp];
        uint nodeIdx = si.node_index;

        // ---- AABB test ------------------------------------------------------
        BvhNode node = bvh_nodes.data[nodeIdx];

        float t_near, t_far;
        if (!intersect_aabb(t_near, t_far, o, inv_d, node.aabb_min, node.aabb_max))
            continue; // completely miss → skip

        // no need to look farther than a hit we already have
        if (t_near > best_hit_depth) continue;

        // ---- Leaf -----------------------------------------------------------
        if (is_leaf(node)) {
            uint octree_offset = fetch_data(node);

            vec3 offset = node.aabb_min;
            vec3 scale  = node.aabb_max - node.aabb_min;

            SvoMarchingResult r = svo_marching(o, d, offset, scale, octree_offset);

            res.total_iter += r.iter;

            // keep the *nearest* real hit
            if (r.is_hit && r.t < best_hit_depth) {
                res.is_hit         = true;
                res.hit_svo_result = r;
                best_hit_depth     = r.t;
            }
            continue;
        }

        // ---- Internal node --------------------------------------------------
        // children are stored *consecutively*: left = node.left, right = left+1
        uint left_child  = node.offset;
        uint right_child = node.offset + 1u;

        // We still want to visit the nearer child first.
        float t_near_l = 1e30, t_near_r = 1e30, dummy_far;
        bool hit_l =
            intersect_aabb(t_near_l, dummy_far, o, inv_d, bvh_nodes.data[left_child].aabb_min,
                           bvh_nodes.data[left_child].aabb_max);

        bool hit_r =
            intersect_aabb(t_near_r, dummy_far, o, inv_d, bvh_nodes.data[right_child].aabb_min,
                           bvh_nodes.data[right_child].aabb_max);

        // push the far child first (LIFO → near processed first)
        if (hit_l && hit_r) {
            uint first     = (t_near_l < t_near_r) ? left_child : right_child;
            uint second    = (t_near_l < t_near_r) ? right_child : left_child;
            float t_first  = min(t_near_l, t_near_r);
            float t_second = max(t_near_l, t_near_r);

            if (t_second < best_hit_depth && sp < STACK_SIZE)
                stack[sp++] = StackInfo(second, si.depth + 1u);
            if (t_first < best_hit_depth && sp < STACK_SIZE)
                stack[sp++] = StackInfo(first, si.depth + 1u);
        } else if (hit_l && t_near_l < best_hit_depth && sp < STACK_SIZE) {
            stack[sp++] = StackInfo(left_child, si.depth + 1u);
        } else if (hit_r && t_near_r < best_hit_depth && sp < STACK_SIZE) {
            stack[sp++] = StackInfo(right_child, si.depth + 1u);
        }
    }

    return res;
}

#endif // BVH_SVO_MARCHING_GLSL
