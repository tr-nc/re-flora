
//! Input: octree_offset_atlas_tex
//! Input: uint data[] inside a buffer named octree_data
#ifndef DDA_SVO_MARCHING_GLSL
#define DDA_SVO_MARCHING_GLSL

#include "./core/definitions.glsl"
#include "./dda_marching.glsl"
#include "./svo_marching.glsl"

struct DdaSvoMarchingResult {
    bool is_hit;
    uint total_iter; // all iterations for all svos
    uint chunk_traversed;
    SvoMarchingResult hit_svo_result;
};

// this marching algorithm fetches leaf properties
DdaSvoMarchingResult dda_svo_marching(ivec3 visible_chunk_dim, vec3 o, vec3 d) {
    DdaSvoMarchingResult cas_result;
    cas_result.is_hit          = false;
    cas_result.total_iter      = 0;
    cas_result.chunk_traversed = 0;

    d = max(abs(d), vec3(EPSILON)) * (step(0.0, d) * 2.0 - 1.0);

    ivec3 map_pos         = ivec3(floor(o));
    const vec3 delta_dist = 1.0 / abs(d);
    const ivec3 ray_step  = ivec3(sign(d));
    vec3 side_dist        = (((sign(d) * 0.5) + 0.5) + sign(d) * (vec3(map_pos) - o)) * delta_dist;
    bool entered_visible_region = false;
    uint dda_iteration          = 0;

    ivec3 chunk_idx;
    while (dda_marching_with_save(chunk_idx, map_pos, side_dist, entered_visible_region,
                                  dda_iteration, visible_chunk_dim, delta_dist, ray_step, o, d)) {
        // pre_offset is to offset the octree tracing hit_pos, which works best with the range of
        // [1, 2]
        uint chunk_buffer_offset = imageLoad(octree_offset_atlas_tex, chunk_idx).x - 1;

        SvoMarchingResult svo_marching_result;
        svo_marching_result = svo_marching(o, d, vec3(chunk_idx), vec3(1.0), chunk_buffer_offset);
        cas_result.total_iter += svo_marching_result.iter;
        cas_result.chunk_traversed++;

        if (svo_marching_result.is_hit) {
            cas_result.is_hit         = true;
            cas_result.hit_svo_result = svo_marching_result;
            return cas_result;
        }
    }
    return cas_result;
}

#endif // DDA_SVO_MARCHING_GLSL
