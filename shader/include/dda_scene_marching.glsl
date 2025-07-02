/// Conducts the DDA marching inside arbitary data structures
/// Requires:
/// - scene_tex: represents the scene in regular chunk sizes
/// - definition of: bool scene_hit(inout MarchingResult o_res, vec3 o, vec3 d, ivec3 map_pos, uvec4
/// scene_tex_read) {}

#ifndef DDA_SCENE_MARCHING_GLSL
#define DDA_SCENE_MARCHING_GLSL

#define MAX_DDA_ITERATION 256

#include "../include/core/aabb.glsl"
#include "../include/core/definitions.glsl"
#include "../include/marching_result.glsl"

MarchingResult dda_scene_marching(vec3 o, vec3 d, vec3 inv_d) {
    MarchingResult res;
    res.iter_count = 0;
    res.is_hit     = false;
    res.pos        = vec3(0.0);
    res.center_pos = vec3(0.0);
    res.t          = 1e10;
    res.normal     = vec3(0.0);
    res.voxel_addr = 0;

    ivec3 visible_chunk_dim = imageSize(scene_tex).xyz;

    vec3 min_bound = vec3(0.0);
    vec3 max_bound = vec3(visible_chunk_dim);

    d = max(abs(d), vec3(EPSILON)) * (step(0.0, d) * 2.0 - 1.0);

    vec2 t = slabs(min_bound, max_bound, o, inv_d);
    // ray shoots out of the entire scene bound directly
    if (t.x > t.y || t.y < 0.0) {
        return res;
    }

    float march_extent  = max(t.x, 0.0) + EPSILON;
    vec3 marched_origin = o + march_extent * d;

    const vec3 delta_dist = 1.0 / abs(d);
    const ivec3 ray_step  = ivec3(sign(d));
    ivec3 map_pos         = ivec3(floor(marched_origin));
    vec3 side_dist =
        (((sign(d) * 0.5) + 0.5) + sign(d) * (vec3(map_pos) - marched_origin)) * delta_dist;

    while (res.iter_count++ < MAX_DDA_ITERATION) {
        bvec3 min_mask = lessThanEqual(side_dist, min(side_dist.yzx, side_dist.zxy));
        side_dist += vec3(min_mask) * delta_dist;
        if (!in_aabb_i(map_pos, ivec3(0), visible_chunk_dim)) {
            break;
        }
        uvec4 scene_tex_read = imageLoad(scene_tex, map_pos);
        if (scene_hit(res, marched_origin, d, map_pos, scene_tex_read)) {
            res.t = length(o - res.pos);
            break;
        }
        map_pos += ivec3(vec3(min_mask)) * ray_step;
    }
    return res;
}

#endif // DDA_SCENE_MARCHING_GLSL
