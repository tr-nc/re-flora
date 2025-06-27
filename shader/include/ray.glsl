#ifndef RAY_GLSL
#define RAY_GLSL

#include "./core/projection.glsl"

struct Ray {
    vec3 origin;
    vec3 direction;
    vec3 inv_direction;
};

Ray ray_gen(vec2 screen_uv) {
    Ray ray;
    ray.origin        = project_screen_uv_to_world_cam_near_point(screen_uv);
    ray.direction     = normalize(project_screen_uv_to_world_cam_far_point(screen_uv) - ray.origin);
    ray.inv_direction = 1.0 / ray.direction;
    return ray;
}

#endif // RAY_GLSL
