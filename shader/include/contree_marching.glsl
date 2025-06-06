#ifndef CONTREE_MARCHING_GLSL
#define CONTREE_MARCHING_GLSL

// Shared stack per work-group invocation
// Notice: the dispatch size is required to be 64 (e.g  8x8x1) for this to work
shared uint gs_stack[64][11];

#include "../include/contree_node.glsl"
#include "../include/core/aabb.glsl"
#include "../include/core/bits.glsl"

struct ContreeMarchingResult {
    bool is_hit;
    vec3 pos;
    vec3 center_pos;
    uint voxel_data;
};

// Reverses pos from [1.0,2.0) to (2.0,1.0] if dir>0
vec3 get_mirrored_pos(vec3 pos, vec3 dir, bool range_check) {
    // although we could reverse float coordinates from range [1.0, 2.0] to [2.0, 1.0] simply by
    // 3.0 - x, our upper bound is exclusive and so this will produce ever so slightly off results,
    // which can cause some minor artifacts if the resulting hit coordinates are used for things
    // like light bounces.
    uvec3 pu      = floatBitsToUint(pos);
    uvec3 flipped = pu ^ uvec3(0x7FFFFFu);
    vec3 mirrored = uintBitsToFloat(flipped);

    // fallback if outside [1,2)
    if (range_check) {
        if (any(lessThan(pos, vec3(1.0))) || any(greaterThanEqual(pos, vec3(2.0)))) {
            mirrored = vec3(3.0) - pos;
        }
    }
    // select per‐component
    return mix(pos, mirrored, greaterThan(dir, vec3(0.0)));
}

// Compute child index [0..26) from bits of pos at this scale
int get_node_cell_index(vec3 pos, int scale_exp) {
    uvec3 pu      = floatBitsToUint(pos);
    uvec3 cellpos = (pu >> uint(scale_exp)) & 3u;
    return int(cellpos.x + cellpos.z * 4u + cellpos.y * 16u);
}

// floor(pos / scale) * scale by zeroing low bits of float bitpattern
vec3 floor_scale(vec3 pos, int scale_exp) {
    uint mask = ~0u << uint(scale_exp);
    uvec3 pu  = floatBitsToUint(pos);
    uvec3 r   = pu & uvec3(mask);
    return uintBitsToFloat(r);
}

/// node_offset is the offset when addressing the contree node data
/// leaf_offset is the offset when addressing the contree leaf data
/// These offsets should be passed in because each chunk are expected to have distinct offsets,
/// which represents there data regions
ContreeMarchingResult _contree_marching(vec3 origin, vec3 dir, bool coarse, uint node_offset,
                                        uint leaf_offset) {
    uint group_id    = gl_LocalInvocationIndex;
    int scale_exp    = 21; // 0.25 (as bit offset in mantissa)
    uint node_idx    = 0u;
    ContreeNode node = contree_node_data.data[node_offset + node_idx];

    ContreeMarchingResult res;
    res.is_hit     = false;
    res.voxel_data = 0;
    res.pos        = vec3(0.0);
    res.center_pos = vec3(0.0);

    vec2 slab = slabs(vec3(1.0), vec3(1.9999999), origin, 1.0 / dir);
    if (slab.x > slab.y || slab.y < 0.0) {
        return res; // out of the broader bound directly
    }
    origin += max(slab.x, 0.0) * dir;

    // Build mirror mask based on ray octant
    uint mirror_mask = 0u;
    if (dir.x > 0.0) mirror_mask |= 3u << 0;
    if (dir.y > 0.0) mirror_mask |= 3u << 4;
    if (dir.z > 0.0) mirror_mask |= 3u << 2;

    origin         = get_mirrored_pos(origin, dir, true);
    vec3 pos       = clamp(origin, 1.0, 1.9999999);
    vec3 inv_dir   = 1.0 / -abs(dir);
    vec3 side_dist = vec3(0.0);

    for (int i = 0; i < 1024; ++i) {
        // optional early‐out for coarse rays
        if (coarse && i > 20 && (node.packed_0 & 1u) != 0u) {
            break;
        }

        uint child_idx = uint(get_node_cell_index(pos, scale_exp)) ^ mirror_mask;

        // Descend as far as possible
        while (((node.child_mask >> uint64_t(child_idx)) & 1u) != 0u &&
               (node.packed_0 & 1u) == 0u) {
            uint stk_idx                = uint(scale_exp >> 1);
            gs_stack[group_id][stk_idx] = node_idx;

            uint bits = bit_count_u64_var(node.child_mask, child_idx);
            node_idx  = (node.packed_0 >> 1u) + bits;
            node      = contree_node_data.data[node_offset + node_idx];

            scale_exp -= 2;
            child_idx = uint(get_node_cell_index(pos, scale_exp)) ^ mirror_mask;
        }

        // if leaf has that child, stop
        if (((node.child_mask >> uint64_t(child_idx)) & 1u) != 0u && (node.packed_0 & 1u) != 0u) {
            break;
        }

        // Figure out how far to step (advance exponent by 1 if no cross‐child)
        int adv_scale_exp = scale_exp;
        if (((node.child_mask >> uint64_t(child_idx & 0x2Au)) & 0x00330033u) == 0u) {
            adv_scale_exp++;
        }

        // Intersect ray with current cell face
        vec3 cell_min = floor_scale(pos, adv_scale_exp);
        side_dist     = (cell_min - origin) * inv_dir;
        float tmax    = min(min(side_dist.x, side_dist.y), side_dist.z);

        // Compute the neighboring cell coordinate
        bvec3 side_mask    = bvec3(tmax >= side_dist.x, tmax >= side_dist.y, tmax >= side_dist.z);
        ivec3 base         = ivec3(floatBitsToInt(cell_min));
        ivec3 off          = ivec3((1 << adv_scale_exp) - 1);
        ivec3 neighbor_max = base + mix(off, ivec3(-1), side_mask);

        // Move to the next cell
        pos = min(origin - abs(dir) * tmax, intBitsToFloat(neighbor_max));

        // If we crossed more than one ancestor level, pop the stack
        uvec3 diff_pos = floatBitsToUint(pos) ^ floatBitsToUint(cell_min);
        uint combined  = (diff_pos.x | diff_pos.y | diff_pos.z) & 0xFFAAAAAAu;
        int diff_exp   = findMSB(int(combined));
        if (diff_exp > scale_exp) {
            scale_exp = diff_exp;
            if (diff_exp > 21) break; // outside root?
            uint stk_idx = uint(scale_exp >> 1);
            node_idx     = gs_stack[group_id][stk_idx];
            node         = contree_node_data.data[node_offset + node_idx];
        }
    }

    // if we ended in a leaf
    if ((node.packed_0 & 1u) != 0u && scale_exp <= 21) {
        res.is_hit = true;

        vec3 centered_pos = floor_scale(pos, scale_exp);
        // this is essentially constructing a float in range [1.0, 2.0) from bit manipulation, then
        // sub 1 see IEEE-754 Floating Point Representation
        float offset = uintBitsToFloat(0x3f800000u | (1u << (scale_exp - 1))) - 1.0;
        centered_pos += offset;

        pos          = get_mirrored_pos(pos, dir, false);
        centered_pos = get_mirrored_pos(centered_pos, dir, false);

        uint child_idx = uint(get_node_cell_index(pos, scale_exp));
        uint bits      = bit_count_u64_var(node.child_mask, child_idx);

        res.pos        = pos;
        res.center_pos = centered_pos;
        res.voxel_data = contree_leaf_data.data[leaf_offset + (node.packed_0 >> 1u) + bits];

        // we need per-voxel normal, so we disable the per-face normal computation here
        // float tmax       = min(min(side_dist.x, side_dist.y), side_dist.z);
        // bvec3 side_mask2 = bvec3(tmax >= side_dist.x, tmax >= side_dist.y, tmax >= side_dist.z);
        // res.normal = vec3(side_mask2.x ? -sign(dir.x) : 0.0, side_mask2.y ? -sign(dir.y) : 0.0,
        //                   side_mask2.z ? -sign(dir.z) : 0.0);
    }

    return res;
}

ContreeMarchingResult contree_marching(vec3 o,              // world-space ray origin
                                       vec3 d,              // world-space ray direction
                                       vec3 chunk_position, // world-space min corner of the chunk
                                       vec3 chunk_scaling,  // size of the chunk along each axis
                                       bool coarse,         // if coarse ray is used
                                       uint node_offset,    // offset in the global node buffer
                                       uint leaf_offset     // offset in the global leaf buffer
) {
    vec3 local_o = (o - chunk_position) / chunk_scaling + 1.0;
    vec3 local_d = d / chunk_scaling;

    ContreeMarchingResult result =
        _contree_marching(local_o, local_d, coarse, node_offset, leaf_offset);

    result.pos        = (result.pos - 1.0) * chunk_scaling + chunk_position;
    result.center_pos = (result.center_pos - 1.0) * chunk_scaling + chunk_position;

    return result;
}

#endif // CONTREE_MARCHING_GLSL
