#ifndef INTERSECT_GLSL
#define INTERSECT_GLSL

/// Returns true if the ray intersects the AABB.
///
/// o_dst_near and o_dst_far are only valid if the function returns true, and
/// represent the distances along the ray where the intersection occurs.
bool intersect_aabb(out float o_dst_near, out float o_dst_far, vec3 ray_origin,
                    vec3 ray_inv_direction, vec3 box_min, vec3 box_max) {
    // compute the intersections with the slabs
    vec3 t_min = (box_min.xyz - ray_origin) * ray_inv_direction;
    vec3 t_max = (box_max.xyz - ray_origin) * ray_inv_direction;
    vec3 t1    = min(t_min, t_max);
    vec3 t2    = max(t_min, t_max);
    o_dst_far  = min(min(t2.x, t2.y), t2.z);
    o_dst_near = max(max(t1.x, t1.y), t1.z); // note this might be negative
    return (o_dst_near <= o_dst_far) && (o_dst_far > 0.);
}

#endif // INTERSECT_GLSL
