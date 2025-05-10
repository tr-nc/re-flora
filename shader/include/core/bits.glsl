//! Extends the GLSL standard library with bit manipulation functions.
//! Utilized some glsl extensions
#ifndef BITS_GLSL
#define BITS_GLSL

#extension GL_ARB_gpu_shader_int64 : enable

// GLSL popcount for uint64
uint bit_count_u64(uint64_t x) { return bitCount(uint(x >> 32)) + bitCount(uint(x)); }

#endif // BITS_GLSL
