#version 450

layout(location = 0) in vec3 in_position_ws;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in uint in_face_id;

layout(location = 0) out vec3 vert_position_ws;
layout(location = 1) out vec2 vert_uv;
layout(location = 2) flat out uint vert_face_id;
layout(location = 3) flat out float vert_near_side;
layout(location = 4) flat out vec2 vert_alpha_pair;

layout(push_constant) uniform PC {
    vec4 box_min_near_alpha;
    vec4 box_max_far_alpha;
}
pc;

layout(set = 0, binding = 3) uniform U_CameraInfo {
    vec4 pos;
    mat4 view_mat;
    mat4 view_mat_inv;
    mat4 proj_mat;
    mat4 proj_mat_inv;
    mat4 view_proj_mat;
    mat4 view_proj_mat_inv;
}
camera_info;

float face_is_near(uint face_id, vec3 camera_from_center) {
    if (face_id == 0u) {
        return camera_from_center.x <= 0.0 ? 1.0 : 0.0;
    }
    if (face_id == 1u) {
        return camera_from_center.x >= 0.0 ? 1.0 : 0.0;
    }
    if (face_id == 2u) {
        return camera_from_center.z <= 0.0 ? 1.0 : 0.0;
    }
    return camera_from_center.z >= 0.0 ? 1.0 : 0.0;
}

void main() {
    vec3 box_min = pc.box_min_near_alpha.xyz;
    vec3 box_max = pc.box_max_far_alpha.xyz;
    vec3 box_center = (box_min + box_max) * 0.5;

    vert_position_ws = in_position_ws;
    vert_uv = in_uv;
    vert_face_id = in_face_id;
    vert_near_side = face_is_near(in_face_id, camera_info.pos.xyz - box_center);
    vert_alpha_pair = vec2(pc.box_min_near_alpha.w, pc.box_max_far_alpha.w);
    gl_Position = camera_info.view_proj_mat * vec4(in_position_ws, 1.0);
}
