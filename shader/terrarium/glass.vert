#version 450

layout(location = 0) in vec3 in_position_ws;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec3 in_normal_ws;
layout(location = 3) in uint in_face_id;
layout(location = 4) in uint in_part_kind;

layout(location = 0) out vec3 vert_position_ws;
layout(location = 1) out vec2 vert_uv;
layout(location = 2) flat out uint vert_face_id;
layout(location = 3) flat out float vert_near_side;
layout(location = 4) flat out vec2 vert_alpha_pair;
layout(location = 5) out vec3 vert_normal_ws;
layout(location = 6) flat out uint vert_part_kind;
layout(location = 7) out vec3 vert_view_dir_ws;

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

void main() {
    vec3 box_min = pc.box_min_near_alpha.xyz;
    vec3 box_max = pc.box_max_far_alpha.xyz;
    vec3 box_center = (box_min + box_max) * 0.5;
    vec3 camera_from_center = camera_info.pos.xyz - box_center;
    vec3 normal_ws = normalize(in_normal_ws);

    vert_position_ws = in_position_ws;
    vert_uv = in_uv;
    vert_face_id = in_face_id;
    vert_near_side = dot(normal_ws, camera_from_center) >= 0.0 ? 1.0 : 0.0;
    vert_alpha_pair = vec2(pc.box_min_near_alpha.w, pc.box_max_far_alpha.w);
    vert_normal_ws = normal_ws;
    vert_part_kind = in_part_kind;
    vert_view_dir_ws = camera_info.pos.xyz - in_position_ws;
    gl_Position = camera_info.view_proj_mat * vec4(in_position_ws, 1.0);
}
