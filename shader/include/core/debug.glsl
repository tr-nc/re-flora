/// Debug utilities
/// Requires:
/// define: DEBUGGING_TEXTURE_NAME
/// define: OUTPUT_TEXTURE_NAME

#ifndef DEBUG_GLSL
#define DEBUG_GLSL

/// Renders the debugging texture in the top-right corner of the screen
/// Returns true if the texture is in the top-right corner, false otherwise
bool debug_texture_in_top_right_corner(ivec2 uvi) {
    ivec2 debugging_tex_size = imageSize(DEBUGGING_TEXTURE_NAME);
    ivec2 debug_region_start = ivec2(imageSize(OUTPUT_TEXTURE_NAME).x - debugging_tex_size.x, 0);
    if (uvi.x >= debug_region_start.x && uvi.y < debugging_tex_size.y) {
        ivec2 debug_tex_uvi  = uvi - debug_region_start;
        vec4 debugging_color = imageLoad(DEBUGGING_TEXTURE_NAME, debug_tex_uvi);
        imageStore(OUTPUT_TEXTURE_NAME, uvi, debugging_color);
        return true;
    }
    return false;
}

#endif // DEBUG_GLSL
