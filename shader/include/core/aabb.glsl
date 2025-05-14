#ifndef AABB_GLSL
#define AABB_GLSL

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

// /// Returns true if the ray intersects the AABB.
// ///
// /// o_dst_near and o_dst_far are only valid if the function returns true, and
// /// represent the distances along the ray where the intersection occurs.
// bool intersect_aabb(out float o_dst_near, out float o_dst_far, vec3 ray_origin,
//                     vec3 ray_inv_direction, vec3 box_min, vec3 box_max) {
//     // compute the intersections with the slabs
//     vec3 t_min = (box_min.xyz - ray_origin) * ray_inv_direction;
//     vec3 t_max = (box_max.xyz - ray_origin) * ray_inv_direction;
//     vec3 t1    = min(t_min, t_max);
//     vec3 t2    = max(t_min, t_max);
//     o_dst_far  = min(min(t2.x, t2.y), t2.z);
//     o_dst_near = max(max(t1.x, t1.y), t1.z); // note this might be negative
//     return (o_dst_near <= o_dst_far) && (o_dst_far > 0.);
// }

bool in_aabb(vec3 point, vec3 box_min, vec3 box_max) {
    return all(greaterThanEqual(point, box_min)) && all(lessThan(point, box_max));
}

bool in_aabb_i(ivec3 point, ivec3 box_min, ivec3 box_max) {
    return all(greaterThanEqual(point, box_min)) && all(lessThan(point, box_max));
}

#endif // AABB_GLSL
