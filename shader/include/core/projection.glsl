#ifndef PROJECTION_GLSL
#define PROJECTION_GLSL

/// Convert screen uv to clip space far point.
/// Vulkan screen uv: (0, 0) is top-left, (1, 1) is bottom-right.
/// We use right-handed coordinate system here, so the z value of clip space
/// far point is -1.
vec4 screen_uv_to_clip_far_point(vec2 screen_uv) {
    // notice that the y axis has been flipped in the projection matrix
    return vec4(screen_uv * 2.0 - 1.0, -1.0, 1.0);
}

/// Convert clip space position to world space position.
vec4 clip_to_world(vec4 clip_pos, mat4 view_proj_mat_inv) {
    vec4 world_pos = view_proj_mat_inv * clip_pos;
    world_pos /= world_pos.w;
    return world_pos;
}

/// Project screen uv to world space camera far point.
vec3 project_screen_uv_to_world_cam_far_point(vec2 screen_uv) {
    vec4 clip_far_point  = screen_uv_to_clip_far_point(screen_uv);
    vec4 world_far_point = clip_to_world(clip_far_point, camera_info.view_proj_mat_inv);
    return world_far_point.xyz;
}

#endif // PROJECTION_GLSL
