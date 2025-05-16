//! Debugging shader time functions
#ifndef SHADER_CLOCK_GLSL
#define SHADER_CLOCK_GLSL

#extension GL_ARB_shader_clock : enable
#extension GL_ARB_gpu_shader_int64 : enable

uint64_t get_current_time() { return clockARB(); }

float get_delta_time(uint64_t start, uint64_t end) { return float(end - start) * 0.001; }

// example usage:
// uint64_t start_time        = get_current_time();
// uint64_t end_time          = get_current_time();
// float delta_time           = get_delta_time(start_time, end_time);
// vec3 time_vis              = inferno_quintic(1.0 - exp(-float(delta_time)));
// vis                        = mix(vis, time_vis, 0.5);

#endif // SHADER_CLOCK_GLSL
