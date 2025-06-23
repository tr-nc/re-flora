#version 450

// Vertex attributes now include height
layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_color;
layout(location = 2) in uint in_height; // The voxel's stack level

// Outputs to fragment shader
layout(location = 0) out vec3 vert_color;

// Existing camera uniforms
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

// New uniform for grass animation parameters
// layout(set = 0, binding = 1) uniform U_GrassInfo {
//     vec2 bend_dir_and_strength;
//     uint voxel_count;
// }
// grass_info;

const vec2 bend_dir_and_strength = vec2(2.0, 0.0);
const uint voxel_count           = 8;

void main() {
    float height = float(in_height);

    // Avoid division by zero if the blade has only one voxel.
    float denom = float(max(voxel_count - 1u, 1u));

    float t       = height / denom;
    float t_curve = t * t; // ease-in curve for a natural bend

    // Calculate the floating-point center of the voxel based on its height and bend
    vec3 voxel_offset =
        vec3(bend_dir_and_strength.x * t_curve, 0.0, bend_dir_and_strength.y * t_curve);

    // Final position is the snapped center of the voxel plus the vertex's local position
    vec3 final_pos = voxel_offset + in_position;

    // Transform to clip space
    gl_Position = camera_info.view_proj_mat * vec4(final_pos, 1.0);

    // Pass color to fragment shader
    vert_color = in_color;
}
