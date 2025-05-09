#ifndef DISPATCH_GROUPER_GLSL
#define DISPATCH_GROUPER_GLSL

uint group_x_64(uint x) { return uint(ceil(float(x) / 64.0)); }
uint group_x_8(uint x) { return uint(ceil(float(x) / 8.0)); }
uint group_x_4(uint x) { return uint(ceil(float(x) / 4.0)); }

#endif // DISPATCH_GROUPER_GLSL
