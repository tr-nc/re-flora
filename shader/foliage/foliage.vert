#version 450

// layout(location = 0) in vec3 in_position;

layout(location = 0) out vec3 vert_color;

// layout(set = 0, binding = 0) uniform U_CameraInfo {
//     vec4 camera_pos; // w is padding.
//     mat4 view_mat;
//     mat4 view_mat_inv;
//     mat4 proj_mat;
//     mat4 proj_mat_inv;
//     mat4 view_proj_mat;
//     mat4 view_proj_mat_inv;
// }
// camera_info;

// We can still pass data to the fragment shader if we want, e.g., for UVs.
// For this simple example, we don't need to.

void main() {
    // This is a common trick to generate a full-screen triangle.
    // The vertices are intentionally oversized to ensure full coverage
    // of the [-1, 1] Normalized Device Coordinate (NDC) space.
    vec2 positions[3] = vec2[](vec2(-1.0, -1.0), vec2(0.0, 1.0), vec2(1.0, -1.0));
    // If your vertices are in model space, the formula would be:
    // view_proj_mat * model_mat * vec4(in_position, 1.0);
    // gl_Position = camera_info.view_proj_mat * vec4(in_position, 1.0);

    gl_Position = vec4(positions[gl_VertexIndex], 0.0, 1.0);

    // For fun, let's pass the world position as a color.
    vert_color = gl_Position.xyz;
}
