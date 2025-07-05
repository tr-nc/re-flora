#ifndef VOXEL_REGISTRY_GLSL
#define VOXEL_REGISTRY_GLSL

// all voxel types

const uint VOXEL_TYPE_EMPTY = 0;
const uint VOXEL_TYPE_SAND  = 1;
const uint VOXEL_TYPE_DIRT  = 2;
const uint VOXEL_TYPE_ROCK  = 3;

const uint VOXEL_TYPE_LEAF  = 4;
const uint VOXEL_TYPE_TRUNK = 6;

// coloring

#include "./core/color.glsl"

vec3 voxel_color_by_type_srgb(uint voxel_type) {
    if (voxel_type == VOXEL_TYPE_EMPTY) {
        return vec3(0.0);
    } else if (voxel_type == VOXEL_TYPE_SAND) {
        return vec3(0.96, 0.87, 0.70);
    } else if (voxel_type == VOXEL_TYPE_DIRT) {
        return vec3(0.29, 0.21, 0.17);
    } else if (voxel_type == VOXEL_TYPE_LEAF) {
        return vec3(0.95, 0.78, 0.14);
    } else if (voxel_type == VOXEL_TYPE_ROCK) {
        return vec3(0.92, 0.36, 0.0);
    } else if (voxel_type == VOXEL_TYPE_TRUNK) {
        return vec3(0.39, 0.29, 0.03);
    }
    return vec3(0.0);
}

vec3 voxel_color_by_type_unorm(uint voxel_type) {
    return srgb_to_linear(voxel_color_by_type_srgb(voxel_type));
}

#endif // VOXEL_REGISTRY_GLSL
