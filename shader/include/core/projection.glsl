#ifndef PROJECTION_GLSL
#define PROJECTION_GLSL

vec4 screen_uv_to_ndc_far_point(vec2 screen_uv) { return vec4(screen_uv * 2.0 - 1.0, 1.0, 1.0); }
vec4 screen_uv_to_ndc_near_point(vec2 screen_uv) { return vec4(screen_uv * 2.0 - 1.0, 0.0, 1.0); }

vec4 ndc_to_world(vec4 ndc_pos, mat4 view_proj_mat_inv) {
    vec4 world_pos = view_proj_mat_inv * ndc_pos;
    world_pos /= world_pos.w;
    return world_pos;
}

vec3 project_screen_uv_to_world_cam_far_point(vec2 screen_uv) {
    vec4 ndc_far_point   = screen_uv_to_ndc_far_point(screen_uv);
    vec4 world_far_point = ndc_to_world(ndc_far_point, camera_info.view_proj_mat_inv);
    return world_far_point.xyz;
}

vec3 project_screen_uv_to_world_cam_near_point(vec2 screen_uv) {
    vec4 ndc_near_point   = screen_uv_to_ndc_near_point(screen_uv);
    vec4 world_near_point = ndc_to_world(ndc_near_point, camera_info.view_proj_mat_inv);
    return world_near_point.xyz;
}

#endif // PROJECTION_GLSL
