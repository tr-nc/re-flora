#ifndef VOXEL_COLOR_GLSL
#define VOXEL_COLOR_GLSL

#include "./core/color.glsl"
#include "./voxel_type.glsl"

// all color pickers are in sRGB space for human preservation, but lighting calculations are done in
// linear space
vec3 voxel_color_by_type_srgb(uint voxel_type) {
    if (voxel_type == VOXEL_TYPE_DIRT) {
        return vec3(0.35, 0.57, 0.23); // Brownish Green
    } else if (voxel_type == VOXEL_TYPE_LEAF) {
        return vec3(0.95, 0.78, 0.14);           // Yellowish (as originally provided)
    } else if (voxel_type == VOXEL_TYPE_CHUNK) { // Assuming this is wood/trunk
        return vec3(0.39, 0.29, 0.03);           // Dark Brown
    } else if (voxel_type == VOXEL_TYPE_SAND) {
        return vec3(0.96, 0.87, 0.70); // Pale Sand
    } else if (voxel_type == VOXEL_TYPE_ROCK) {
        return vec3(0.50, 0.50, 0.50); // Medium Gray
    } else if (voxel_type == VOXEL_TYPE_EMPTY) {
        // empty voxel shouldn't be rendered at all, returning black as a fallback.
        return vec3(0.0, 0.0, 0.0);
    }
    // Default for any other unknown voxel type
    return vec3(0.0, 0.0, 0.0); // Return black
}

vec3 voxel_color_by_type_unorm(uint voxel_type) {
    return srgb_to_linear(voxel_color_by_type_srgb(voxel_type));
}

#endif // VOXEL_COLOR_GLSL
