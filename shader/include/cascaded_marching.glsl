//! Input: octree_offset_atlas_tex
//! Input: uint data[] inside a buffer named octree_data
#ifndef CASCADED_MARCHING_GLSL
#define CASCADED_MARCHING_GLSL

#include "../include/chunking.glsl"
#include "../include/core/definitions.glsl"
#include "../include/dda_marching.glsl"
#include "../include/svo_marching.glsl"

struct CascadedMarchingResult {
    bool is_hit;
    uint total_iter; // all iterations for all svos
    uint chunk_traversed;
    SvoMarchingResult last_hit_svo_result;
};

uint _read_octree_offset(ivec3 chunk_idx) {
    return imageLoad(octree_offset_atlas_tex, chunk_idx).x;
}

// this marching algorithm fetches leaf properties
MarchingResult cascaded_marching(ivec3 visible_chunk_dim, vec3 o, vec3 d) {
    MarchingResult cas_result;
    cas_result.is_hit = false;

    d = max(abs(d), vec3(EPSILON)) * (step(0.0, d) * 2.0 - 1.0);

    ivec3 map_pos         = ivec3(floor(o));
    const vec3 delta_dist = 1.0 / abs(d);
    const ivec3 ray_step  = ivec3(sign(d));
    vec3 side_dist        = (((sign(d) * 0.5) + 0.5) + sign(d) * (vec3(map_pos) - o)) * delta_dist;
    bool entered_visible_region = false;
    uint dda_iteration          = 0;

    ivec3 chunk_idx;
    while (dda_marching_with_save(chunk_idx, map_pos, side_dist, entered_visible_region,
                                  dda_iteration, delta_dist, ray_step, o, d)) {
        // pre_offset is to offset the octree tracing hit_pos, which works best with the range of
        // [1, 2]
        uint chunk_buffer_offset = chunk_indices_buffer.data[getChunksBufferLinearIndex(
                                       uvec3(chunk_idx), sceneInfoBuffer.data.chunksDim)] -
                                   1;

        const vec3 pre_offset = -chunk_idx;

        SvoMarchingResult svo_result;
        svo_result = svo_marching(o + pre_offset, d, chunk_buffer_offset);
        svo_result.hit_pos -= pre_offset;
        svo_result.next_ray_start_pos -= pre_offset;

        cas_result.iter += chunk_iter_count;
        cas_result.chunk_traversed++;
        if (svo_result.is_hit) {
            cas_result.t                = t;
            cas_result.color            = color;
            cas_result.hit_pos          = pos - origin_offset;
            cas_result.next_tracing_pos = next_tracing_pos - origin_offset;
            cas_result.normal           = normal;
            cas_result.vox_hash         = vox_hash;
            return true;
        }
    }
    return false;
}

#endif // CASCADED_MARCHING_GLSL
