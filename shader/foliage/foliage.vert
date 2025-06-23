#version 450

// these are vertex-rate attributes
layout(location = 0) in vec3 in_position;
layout(location = 1) in vec3 in_color;
layout(location = 2) in uint in_height; // The voxel's stack level

// these are instance-rate attributes
// this should match the grass_instance.glsl
layout(location = 3) in uvec3 in_instance_position;
layout(location = 4) in uint in_instance_grass_type;

layout(location = 0) out vec3 vert_color;

layout(set = 0, binding = 0) uniform U_CameraInfo {
    vec4 camera_pos;
    mat4 view_mat;
    mat4 view_mat_inv;
    mat4 proj_mat;
    mat4 proj_mat_inv;
    mat4 view_proj_mat;
    mat4 view_proj_mat_inv;
}
camera_info;

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
    vec4 final_pos = vec4(in_position + voxel_offset + in_instance_position, 1.0);

    // create a scale matrix
    mat4 scale_mat  = mat4(1.0);
    scale_mat[0][0] = 1.0 / 256.0;
    scale_mat[1][1] = 1.0 / 256.0;
    scale_mat[2][2] = 1.0 / 256.0;

    final_pos = (scale_mat * final_pos);

    // Transform to clip space
    gl_Position = camera_info.view_proj_mat * final_pos;

    // Pass color to fragment shader
    vert_color = in_color;
}
