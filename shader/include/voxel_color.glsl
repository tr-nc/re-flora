#ifndef VOXEL_COLOR_GLSL
#define VOXEL_COLOR_GLSL

#include "./core/color.glsl"
#include "./voxel_type.glsl"

vec3 voxel_color_by_type_srgb(uint voxel_type) {
    if (voxel_type == VOXEL_TYPE_EMPTY) {
        return vec3(0.0);
    } else if (voxel_type == VOXEL_TYPE_SAND) {
        return vec3(0.96, 0.87, 0.70);
    } else if (voxel_type == VOXEL_TYPE_DIRT) {
        return vec3(0.35, 0.57, 0.23);
    } else if (voxel_type == VOXEL_TYPE_GRASS) {
        return vec3(0.30, 0.70, 0.25);
    } else if (voxel_type == VOXEL_TYPE_LEAF) {
        return vec3(0.95, 0.78, 0.14);
    } else if (voxel_type == VOXEL_TYPE_ROCK) {
        return vec3(0.50, 0.50, 0.50);
    } else if (voxel_type == VOXEL_TYPE_CHUNK) {
        return vec3(0.39, 0.29, 0.03);
    }
    return vec3(0.0);
}

vec3 voxel_color_by_type_unorm(uint voxel_type) {
    return srgb_to_linear(voxel_color_by_type_srgb(voxel_type));
}

#endif // VOXEL_COLOR_GLSL