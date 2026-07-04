#ifndef FLORA_PUSH_CONSTANT_GLSL
#define FLORA_PUSH_CONSTANT_GLSL

// Twelve rows cover the tallest authored surface flora currently generated
// (ember bloom, 12 vertical layers).  Each row stores a precomputed linear RGB
// color packed as 10 bits/channel, so the vertex shader samples height color by
// table index instead of interpolating bottom/tip endpoints.
const int FLORA_HEIGHT_COLOR_TABLE_LEN = 12;

layout(push_constant) uniform PC {
    float time;
    uint instance_ty;
    uvec3 chunk_world_offset;
    uint _padding_after_chunk_world_offset;
    uint height_dark_color_rgb10[FLORA_HEIGHT_COLOR_TABLE_LEN];
    uint height_light_color_rgb10[FLORA_HEIGHT_COLOR_TABLE_LEN];
}
pc;

#endif // FLORA_PUSH_CONSTANT_GLSL
