//! Input: octree_offset_atlas_tex
#ifndef DDA_MARCHING_GLSL
#define DDA_MARCHING_GLSL

#define MAX_DDA_ITERATION 50

bool _in_chunk_range(ivec3 pos, ivec3 visible_chunk_dim) {
    return all(greaterThanEqual(pos, ivec3(0))) && all(lessThan(pos, visible_chunk_dim));
}

uint _read_octree_offset(ivec3 chunk_idx) {
    return imageLoad(octree_offset_atlas_tex, chunk_idx).x;
}

bool _has_chunk(ivec3 chunk_idx) { return _read_octree_offset(chunk_idx) != 0; }

// this function if used for continuous raymarching, where we need to save the last hit chunk
bool dda_marching_with_save(out ivec3 o_chunk_idx, inout ivec3 map_pos, inout vec3 side_dist,
                            inout bool entered_visible_region, inout uint it,
                            ivec3 visible_chunk_dim, vec3 delta_dist, ivec3 ray_step, vec3 o,
                            vec3 d) {
    bvec3 mask;
    while (it++ < MAX_DDA_ITERATION) {
        mask = lessThanEqual(side_dist.xyz, min(side_dist.yzx, side_dist.zxy));
        side_dist += vec3(mask) * delta_dist;

        o_chunk_idx = map_pos;
        map_pos += ivec3(vec3(mask)) * ray_step;

        if (_in_chunk_range(o_chunk_idx, visible_chunk_dim)) {
            entered_visible_region = true;
            if (_has_chunk(o_chunk_idx)) {
                return true;
            }
        }
        // went outside the outer bounding box
        else if (entered_visible_region) {
            return false;
        }
    }
    return false;
}

#endif // DDA_MARCHING_GLSL
