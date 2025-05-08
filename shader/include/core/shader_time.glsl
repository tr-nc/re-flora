//! Debugging shader time functions
#ifndef SHADER_TIME_GLSL
#define SHADER_TIME_GLSL

#extension GL_ARB_shader_clock : enable

uvec2 get_current_time() { return clock2x32ARB(); }

float get_delta_time(uvec2 start, uvec2 end) {
    // Low-word difference and borrow flag
    uint delta_low = end.x - start.x;
    uint borrow    = end.x < start.x ? 1u : 0u;
    // High-word difference minus any borrow
    uint delta_high = end.y - start.y - borrow;

    // Reconstruct full 64-bit delta as a float:
    //   ticks = delta_low + delta_high * 2^32
    // Note: 2^32 = 4294967296.0
    float ticks = float(delta_low) + float(delta_high) * 4294967296.0;

    return ticks * 0.001;
}

#endif // SHADER_TIME_GLSL
