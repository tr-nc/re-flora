#version 450

layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_color;

layout(location = 0) out vec3 vert_color;

layout(set = 0, binding = 0) uniform U_CameraInfo {
    vec4 camera_pos; // w is padding.
    mat4 view_mat;
    mat4 view_mat_inv;
    mat4 proj_mat;
    mat4 proj_mat_inv;
    mat4 view_proj_mat;
    mat4 view_proj_mat_inv;
}
camera_info;

// These are your model-space positions for a test triangle
// const vec3 positions[3] = vec3[](vec3(-1.0, -1.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(1.0, -1.0,
// 0.0));

void main() {
    // Take the model-space position and transform it once to clip space.
    // gl_Position = camera_info.view_proj_mat * vec4(positions[gl_VertexIndex], 1.0);

    // Transform the input model-space position to clip space.
    gl_Position = camera_info.view_proj_mat * vec4(in_position, 1.0);

    // Pass the original model-space position as the color for visualization.
    vert_color = in_color;
}
