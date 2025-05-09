//! Debugging shader time functions
#ifndef SHADER_CLOCK_GLSL
#define SHADER_CLOCK_GLSL

#extension GL_ARB_shader_clock : enable
#extension GL_ARB_gpu_shader_int64 : enable

uint64_t get_current_time() { return clockARB(); }

float get_delta_time(uint64_t start, uint64_t end) { return float(end - start) * 0.001; }

#endif // SHADER_CLOCK_GLSL
