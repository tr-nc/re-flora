#ifndef GRASS_COLOR_GLSL
#define GRASS_COLOR_GLSL

#include "./core/color.glsl"
#include "./grass_type.glsl"

// all color pickers are in sRGB space for human preservation, but lighting calculations are done in
// linear space
vec3 grass_color_by_type_srgb(uint grass_type) {
    if (grass_type == GRASS_TYPE_NORMAL) {
        return vec3(0.255, 0.596, 0.039);
    }
    // empty voxel shouldn't be rendered at all
    return vec3(0.0);
}

vec3 grass_color_by_type_unorm(uint grass_type) {
    return srgb_to_linear(grass_color_by_type_srgb(grass_type));
}

#endif // GRASS_COLOR_GLSL
