#ifndef RAY_GLSL
#define RAY_GLSL

#include "./core/projection.glsl"

struct Ray {
    vec3 origin;
    vec3 direction;
    vec3 inv_direction;
};

Ray ray_gen(vec3 camera_pos, vec2 screen_uv) {
    Ray ray;
    ray.origin        = camera_pos;
    ray.direction     = normalize(project_screen_uv_to_world_cam_far_point(screen_uv) - camera_pos);
    ray.inv_direction = 1.0 / ray.direction;
    return ray;
}

#endif // RAY_GLSL
