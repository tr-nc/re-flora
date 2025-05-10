#ifndef MATH_GLSL
#define MATH_GLSL

// x must be guaranteed to be a power of 4
uint log_4(uint x) { return uint(findMSB(x)) >> 1; }

#endif // MATH_GLSL
