#ifndef PROJECTION_GLSL
#define PROJECTION_GLSL

vec4 screen_uv_to_ndc_far_point(vec2 screen_uv) { return vec4(screen_uv * 2.0 - 1.0, 1.0, 1.0); }
vec4 screen_uv_to_ndc_near_point(vec2 screen_uv) { return vec4(screen_uv * 2.0 - 1.0, 0.0, 1.0); }

vec3 ndc_to_world(vec4 ndc_pos, mat4 view_proj_mat_inv) {
    vec4 world_pos = view_proj_mat_inv * ndc_pos;
    world_pos /= world_pos.w;
    return world_pos.xyz;
}

vec3 project_screen_uv_to_world_cam_far_point(vec2 screen_uv, mat4 view_proj_mat_inv) {
    vec4 ndc_far_point   = screen_uv_to_ndc_far_point(screen_uv);
    vec3 world_far_point = ndc_to_world(ndc_far_point, view_proj_mat_inv);
    return world_far_point;
}

vec3 project_screen_uv_to_world_cam_near_point(vec2 screen_uv, mat4 view_proj_mat_inv) {
    vec4 ndc_near_point   = screen_uv_to_ndc_near_point(screen_uv);
    vec3 world_near_point = ndc_to_world(ndc_near_point, view_proj_mat_inv);
    return world_near_point;
}

vec2 project_world_to_screen(vec3 world_pos, mat4 view_proj_mat) {
    vec4 screen_box_coord = view_proj_mat * vec4(world_pos, 1.0);
    screen_box_coord /= screen_box_coord.w;
    vec2 screen_uv = screen_box_coord.xy;
    screen_uv      = (screen_uv + 1.0) * 0.5;
    return screen_uv;
}

#endif // PROJECTION_GLSL
