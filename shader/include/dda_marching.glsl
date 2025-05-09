#ifndef DDA_MARCHING_GLSL
#define DDA_MARCHING_GLSL

#define MAX_DDA_ITERATION 256

#include "../include/core/definitions.glsl"

// intersects if: t_min < t_max && t_max > 0.0
// t_min is the first intersection point, t_max is the last intersection point
// t_min might be negative, which means the ray starts inside the box
// this function is highly optimized and is used in industry standard
vec2 slabs(vec3 p0, vec3 p1, vec3 o, vec3 inv_d) {
    vec3 t0 = (p0 - o) * inv_d;
    vec3 t1 = (p1 - o) * inv_d;

    vec3 temp = t0;
    t0 = min(temp, t1), t1 = max(temp, t1);

    float t_min = max(max(t0.x, t0.y), t0.z);
    float t_max = min(min(t1.x, t1.y), t1.z);

    return vec2(t_min, t_max);
}

bool _in_chunk_range(ivec3 pos, ivec3 visible_chunk_dim) {
    return all(greaterThanEqual(pos, ivec3(0))) && all(lessThan(pos, visible_chunk_dim));
}

uint dda_svo_marching(ivec3 visible_chunk_dim, vec3 o, vec3 d, vec3 inv_d) {
    vec3 min_bound = vec3(0.0);
    vec3 max_bound = vec3(visible_chunk_dim);

    d = max(abs(d), vec3(EPSILON)) * (step(0.0, d) * 2.0 - 1.0);

    vec2 t = slabs(min_bound, max_bound, o, inv_d);
    if (t.x > t.y || t.y < 0.0) {
        return 0;
    }

    float march_extent = max(t.x, 0.0) + EPSILON;
    o += march_extent * d;

    ivec3 map_pos   = ivec3(floor(o));
    vec3 delta_dist = 1.0 / abs(d);
    ivec3 ray_step  = ivec3(sign(d));
    vec3 side_dist  = (((sign(d) * 0.5) + 0.5) + sign(d) * (vec3(map_pos) - o)) * delta_dist;

    uint iter_count = 0;
    while (iter_count++ < MAX_DDA_ITERATION) {
        bvec3 min_mask = lessThanEqual(side_dist, min(side_dist.yzx, side_dist.zxy));
        side_dist += vec3(min_mask) * delta_dist;
        if (!_in_chunk_range(map_pos, visible_chunk_dim)) {
            break;
        }
        map_pos += ivec3(vec3(min_mask)) * ray_step;
    }
    return iter_count;
}
#endif // DDA_MARCHING_GLSL
