#ifndef COLOR_GLSL
#define COLOR_GLSL

float lum(vec3 col) { return dot(col, vec3(0.2126, 0.7152, 0.0722)); }

#endif // COLOR_GLSL
