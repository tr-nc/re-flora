#ifndef DDA_SCENE_MARCHING_GLSL
#define DDA_SCENE_MARCHING_GLSL

#define MAX_DDA_ITERATION 256

#include "../include/core/aabb.glsl"
#include "../include/core/definitions.glsl"
#include "../include/dda_scene_marching_result.glsl"

DdaSceneMarchingResult dda_scene_marching(vec3 o, vec3 d, vec3 inv_d) {
    DdaSceneMarchingResult res;
    res.iter_count = 0;
    res.is_hit     = false;
    res.pos        = vec3(0.0);
    res.normal     = vec3(0.0);
    res.voxel_data = 0;

    ivec3 visible_chunk_dim = imageSize(SCENE_TEX_NAME).xyz;

    vec3 min_bound = vec3(0.0);
    vec3 max_bound = vec3(visible_chunk_dim);

    d = max(abs(d), vec3(EPSILON)) * (step(0.0, d) * 2.0 - 1.0);

    vec2 t = slabs(min_bound, max_bound, o, inv_d);
    // ray shoots out of the scene bound directly
    if (t.x > t.y || t.y < 0.0) {
        return res;
    }

    float march_extent = max(t.x, 0.0) + EPSILON;
    o += march_extent * d;

    const vec3 delta_dist = 1.0 / abs(d);
    const ivec3 ray_step  = ivec3(sign(d));
    ivec3 map_pos         = ivec3(floor(o));
    vec3 side_dist        = (((sign(d) * 0.5) + 0.5) + sign(d) * (vec3(map_pos) - o)) * delta_dist;

    while (res.iter_count++ < MAX_DDA_ITERATION) {
        bvec3 min_mask = lessThanEqual(side_dist, min(side_dist.yzx, side_dist.zxy));
        side_dist += vec3(min_mask) * delta_dist;
        if (!in_aabb_i(map_pos, ivec3(0), visible_chunk_dim)) {
            break;
        }
        uvec4 scene_tex_read = imageLoad(SCENE_TEX_NAME, map_pos);
        if (scene_hit(res, o, d, map_pos, scene_tex_read)) {
            break;
        }
        map_pos += ivec3(vec3(min_mask)) * ray_step;
    }
    return res;
}

#endif // DDA_SCENE_MARCHING_GLSL
