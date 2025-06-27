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

const uint voxel_count     = 8;
const float scaling_factor = 1.0 / 256.0;

vec3 get_offset_of_vertex(float voxel_height, uint voxel_count, vec2 grass_offset) {
    // Avoid division by zero if the blade has only one voxel.
    float denom = float(max(voxel_count - 1u, 1u));

    float t       = voxel_height / denom;
    float t_curve = t * t; // ease-in curve for a natural bend

    // Calculate the floating-point center of the voxel based on its height and bend
    return vec3(grass_info.grass_offset.x * t_curve, 0.0, grass_info.grass_offset.y * t_curve);
}

void get_shadow_weight(out float o_shadow_weight, out bool o_shadow_result_valid,
                       vec4 voxel_pos_ws) {
    vec4 point_ndc = shadow_camera_info.view_proj_mat * voxel_pos_ws;
    vec2 shadow_uv = point_ndc.xy / point_ndc.w;
    shadow_uv      = shadow_uv * 0.5 + 0.5;

    o_shadow_result_valid =
        all(lessThanEqual(shadow_uv, vec2(1.0))) && all(greaterThanEqual(shadow_uv, vec2(0.0)));
    if (!o_shadow_result_valid) {
        o_shadow_weight = 0.0;
        return;
    }

    float shadow_depth = texture(shadow_map_tex, shadow_uv).r;
    float depth_01     = point_ndc.z / point_ndc.w;
    float delta        = depth_01 - shadow_depth;
    bool is_in_shadow  = delta > 0.001;
    float weight_01    = is_in_shadow ? 0.0 : 1.0;

    o_shadow_weight = weight_01;
}

void main() {
    float height       = float(in_height);
    vec3 vertex_offset = get_offset_of_vertex(height, voxel_count, grass_info.grass_offset);
    vec3 vert_pos_ms   = in_position + vertex_offset;
    vec4 vert_pos_ws   = vec4(vert_pos_ms + in_instance_position, 1.0);
    vec3 voxel_pos_ms  = float(in_height) + vec3(0.5) + vertex_offset;
    vec4 voxel_pos_ws  = vec4(voxel_pos_ms + in_instance_position, 1.0);

    mat4 scale_mat  = mat4(1.0);
    scale_mat[0][0] = scaling_factor;
    scale_mat[1][1] = scaling_factor;
    scale_mat[2][2] = scaling_factor;
    vert_pos_ws     = (scale_mat * vert_pos_ws);
    voxel_pos_ws    = (scale_mat * voxel_pos_ws);

    float shadow_weight;
    bool shadow_result_valid;
    get_shadow_weight(shadow_weight, shadow_result_valid, voxel_pos_ws);

    // transform to clip space
    gl_Position = camera_info.view_proj_mat * vert_pos_ws;

    float ambient_light = 0.2;
    // if out of shadow map range, vert_color is red to warn
    if (!shadow_result_valid) {
        vert_color = vec3(1.0, 0.0, 0.0);
    } else {
        vert_color = in_color * (shadow_weight + ambient_light);
    }
}
