#ifndef VOXEL_COLORS_GLSL
#define VOXEL_COLORS_GLSL

#include "./core/color.glsl"

layout(set = 0, binding = 11) uniform U_VoxelColors {
    vec3 sand_color;
    vec3 dirt_color;
    vec3 rock_color;
    vec3 leaf_color;
    vec3 trunk_color;
}
voxel_colors;

vec3 voxel_color_by_type_srgb(uint voxel_type) {
    if (voxel_type == VOXEL_TYPE_EMPTY) {
        return vec3(0.0);
    } else if (voxel_type == VOXEL_TYPE_SAND) {
        return voxel_colors.sand_color;
    } else if (voxel_type == VOXEL_TYPE_DIRT) {
        return voxel_colors.dirt_color;
    } else if (voxel_type == VOXEL_TYPE_LEAF) {
        return voxel_colors.leaf_color;
    } else if (voxel_type == VOXEL_TYPE_ROCK) {
        return voxel_colors.rock_color;
    } else if (voxel_type == VOXEL_TYPE_TRUNK) {
        return voxel_colors.trunk_color;
    }
    return vec3(0.0);
}

vec3 voxel_color_by_type_unorm(uint voxel_type) {
    return srgb_to_linear(voxel_color_by_type_srgb(voxel_type));
}

#endif // VOXEL_COLORS_GLSL