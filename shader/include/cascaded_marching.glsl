//! Input: octree_offset_atlas_tex
//! Input: uint data[] inside a buffer named octree_data
#ifndef CASCADED_MARCHING_GLSL
#define CASCADED_MARCHING_GLSL

#include "../include/chunking.glsl"
#include "../include/dda_marching.glsl"
#include "../include/svo_marching.glsl"

struct MarchingResult {
    uint iter;
    uint chunk_traversed;
    float t;
    vec3 color;
    vec3 position;
    vec3 next_tracing_pos;
    vec3 normal;
    uint vox_hash;
    bool light_source_hit;
};

uint _read_octree_offset(ivec3 chunk_idx) {
    return imageLoad(octree_offset_atlas_tex, chunk_idx).x;
}

// this marching algorithm fetches leaf properties
bool cascaded_marching(out MarchingResult o_result, ivec3 visible_chunk_dim, vec3 o, vec3 d) {
    ivec3 chunk_idx;
    bool hit_voxel = false;

    o_result.iter             = 0;
    o_result.chunk_traversed  = 0;
    o_result.t                = 1e10;
    o_result.color            = vec3(0);
    o_result.position         = o + d * o_result.t;
    o_result.next_tracing_pos = o_result.position;
    o_result.normal           = vec3(0);
    o_result.vox_hash         = 0;
    o_result.light_source_hit = false;

    d = max(abs(d), vec3(kEpsilon)) * (step(0.0, d) * 2.0 - 1.0);

    ivec3 mapPos          = ivec3(floor(o));
    const vec3 delta_dist = 1.0 / abs(d);
    const ivec3 ray_step  = ivec3(sign(d));
    vec3 side_dist        = (((sign(d) * 0.5) + 0.5) + sign(d) * (vec3(mapPos) - o)) * delta_dist;
    bool entered_visible_region = false;
    uint dda_iteration          = 0;
    while (dda_marching_with_save(chunk_idx, mapPos, side_dist, entered_visible_region,
                                  dda_iteration, delta_dist, ray_step, o, d)) {
        // pre_offset is to offset the octree tracing position, which works best with the range of
        // [1, 2]
        const ivec3 pre_offset   = ivec3(1);
        const vec3 origin_offset = pre_offset - chunk_idx;

        uint chunk_buffer_offset = chunk_indices_buffer.data[getChunksBufferLinearIndex(
                                       uvec3(chunk_idx), sceneInfoBuffer.data.chunksDim)] -
                                   1;

        uint chunk_iter_count, vox_hash;
        vec3 color, pos, next_tracing_pos, normal;
        bool light_source_hit;
        float t;
        hit_voxel =
            svo_marching(t, chunk_iter_count, color, pos, next_tracing_pos, normal, vox_hash,
                         light_source_hit, o + origin_offset, d, chunk_buffer_offset);

        o_result.iter += chunk_iter_count;
        o_result.chunk_traversed++;
        if (hit_voxel) {
            o_result.t                = t;
            o_result.color            = color;
            o_result.position         = pos - origin_offset;
            o_result.next_tracing_pos = next_tracing_pos - origin_offset;
            o_result.normal           = normal;
            o_result.vox_hash         = vox_hash;
            o_result.light_source_hit = light_source_hit;
            return true;
        }
    }
    return false;
}

#endif // CASCADED_MARCHING_GLSL
