#ifndef VOXEL_TYPE_GLSL
#define VOXEL_TYPE_GLSL

const uint VOXEL_TYPE_EMPTY      = 0;
const uint VOXEL_TYPE_GRASS_LAND = 1;
const uint VOXEL_TYPE_LEAF       = 2;
const uint VOXEL_TYPE_CHUNK      = 3;

// const uint VOXEL_TYPE_MAX = 3;

// https://colorhunt.co/palette/5b913b77b254ffe8b6d99d81
// all color pickers are in sRGB space for human preservation, but lighting calculations are done in
// linear space
vec3 voxel_srgb_color_by_type(uint voxel_type) {
    if (voxel_type == VOXEL_TYPE_GRASS_LAND) {
        return vec3(0.35, 0.57, 0.23);
    } else if (voxel_type == VOXEL_TYPE_LEAF) {
        return vec3(0.1, 0.5, 0.25);
    } else if (voxel_type == VOXEL_TYPE_CHUNK) {
        return vec3(0.65, 0.4, 0.27);
    }
    // empty voxel shouldn't be rendered at all
    return vec3(0.0);
}

#endif // VOXEL_TYPE_GLSL
