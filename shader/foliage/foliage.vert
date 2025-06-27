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
    vec4 pos;
    mat4 view_mat;
    mat4 view_mat_inv;
    mat4 proj_mat;
    mat4 proj_mat_inv;
    mat4 view_proj_mat;
    mat4 view_proj_mat_inv;
}
camera_info;

layout(set = 0, binding = 1) uniform U_ShadowCameraInfo {
    vec4 pos;
    mat4 view_mat;
    mat4 view_mat_inv;
    mat4 proj_mat;
    mat4 proj_mat_inv;
    mat4 view_proj_mat;
    mat4 view_proj_mat_inv;
}
shadow_camera_info;

layout(set = 0, binding = 2) uniform U_GrassInfo { vec2 grass_offset; }
grass_info;

layout(set = 0, binding = 3) uniform sampler2D shadow_map_tex;

const uint voxel_count = 8;

vec3 get_offset_of_vertex(float voxel_height, uint voxel_count, vec2 grass_offset) {
    // Avoid division by zero if the blade has only one voxel.
    float denom = float(max(voxel_count - 1u, 1u));

    float t       = voxel_height / denom;
    float t_curve = t * t; // ease-in curve for a natural bend

    // Calculate the floating-point center of the voxel based on its height and bend
    return vec3(grass_info.grass_offset.x * t_curve, 0.0, grass_info.grass_offset.y * t_curve);
}

void main() {
    float height         = float(in_height);
    vec3 vertex_offset   = get_offset_of_vertex(height, voxel_count, grass_info.grass_offset);
    vec3 model_space_pos = in_position + vertex_offset;
    vec4 world_space_pos = vec4(model_space_pos + in_instance_position, 1.0);

    mat4 scale_mat  = mat4(1.0);
    scale_mat[0][0] = 1.0 / 256.0;
    scale_mat[1][1] = 1.0 / 256.0;
    scale_mat[2][2] = 1.0 / 256.0;
    world_space_pos = (scale_mat * world_space_pos);

    // Transform to clip space
    gl_Position = camera_info.view_proj_mat * world_space_pos;

    // Pass color to fragment shader
    vert_color = in_color;
}
