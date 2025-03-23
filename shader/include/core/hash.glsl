#ifndef HASH_GLSL
#define HASH_GLSL

// Hash Functions
// taken from: https://www.shadertoy.com/view/ttc3zr
//
// murmurHashNM() takes M unsigned integers and returns N hash values.
// The returned values are unsigned integers between 0 and 2^32 - 1.
//
// hashNM() takes M floating point numbers and returns N hash values.
// The returned values are floating point numbers between 0.0 and 1.0.

//------------------------------------------------------------------------------

uint murmurHash11(uint src) {
  const uint M = 0x5bd1e995u;
  uint h       = 1190494759u;
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 1 output, 1 input
float hash11(float src) {
  uint h = murmurHash11(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uint murmurHash12(uvec2 src) {
  const uint M = 0x5bd1e995u;
  uint h       = 1190494759u;
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src.x;
  h *= M;
  h ^= src.y;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 1 output, 2 inputs
float hash12(vec2 src) {
  uint h = murmurHash12(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uint murmurHash13(uvec3 src) {
  const uint M = 0x5bd1e995u;
  uint h       = 1190494759u;
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src.x;
  h *= M;
  h ^= src.y;
  h *= M;
  h ^= src.z;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 1 output, 3 inputs
float hash13(vec3 src) {
  uint h = murmurHash13(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uint murmurHash14(uvec4 src) {
  const uint M = 0x5bd1e995u;
  uint h       = 1190494759u;
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src.x;
  h *= M;
  h ^= src.y;
  h *= M;
  h ^= src.z;
  h *= M;
  h ^= src.w;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 1 output, 4 inputs
float hash14(vec4 src) {
  uint h = murmurHash14(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec2 murmurHash21(uint src) {
  const uint M = 0x5bd1e995u;
  uvec2 h      = uvec2(1190494759u, 2147483647u);
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 2 outputs, 1 input
vec2 hash21(float src) {
  uvec2 h = murmurHash21(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec2 murmurHash22(uvec2 src) {
  const uint M = 0x5bd1e995u;
  uvec2 h      = uvec2(1190494759u, 2147483647u);
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src.x;
  h *= M;
  h ^= src.y;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 2 outputs, 2 inputs
vec2 hash22(vec2 src) {
  uvec2 h = murmurHash22(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec2 murmurHash23(uvec3 src) {
  const uint M = 0x5bd1e995u;
  uvec2 h      = uvec2(1190494759u, 2147483647u);
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src.x;
  h *= M;
  h ^= src.y;
  h *= M;
  h ^= src.z;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 2 outputs, 3 inputs
vec2 hash23(vec3 src) {
  uvec2 h = murmurHash23(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec2 murmurHash24(uvec4 src) {
  const uint M = 0x5bd1e995u;
  uvec2 h      = uvec2(1190494759u, 2147483647u);
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src.x;
  h *= M;
  h ^= src.y;
  h *= M;
  h ^= src.z;
  h *= M;
  h ^= src.w;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 2 outputs, 4 inputs
vec2 hash24(vec4 src) {
  uvec2 h = murmurHash24(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec3 murmurHash31(uint src) {
  const uint M = 0x5bd1e995u;
  uvec3 h      = uvec3(1190494759u, 2147483647u, 3559788179u);
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 3 outputs, 1 input
vec3 hash31(float src) {
  uvec3 h = murmurHash31(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec3 murmurHash32(uvec2 src) {
  const uint M = 0x5bd1e995u;
  uvec3 h      = uvec3(1190494759u, 2147483647u, 3559788179u);
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src.x;
  h *= M;
  h ^= src.y;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 3 outputs, 2 inputs
vec3 hash32(vec2 src) {
  uvec3 h = murmurHash32(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec3 murmurHash33(uvec3 src) {
  const uint M = 0x5bd1e995u;
  uvec3 h      = uvec3(1190494759u, 2147483647u, 3559788179u);
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src.x;
  h *= M;
  h ^= src.y;
  h *= M;
  h ^= src.z;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 3 outputs, 3 inputs
vec3 hash33(vec3 src) {
  uvec3 h = murmurHash33(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec3 murmurHash34(uvec4 src) {
  const uint M = 0x5bd1e995u;
  uvec3 h      = uvec3(1190494759u, 2147483647u, 3559788179u);
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src.x;
  h *= M;
  h ^= src.y;
  h *= M;
  h ^= src.z;
  h *= M;
  h ^= src.w;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 3 outputs, 4 inputs
vec3 hash34(vec4 src) {
  uvec3 h = murmurHash34(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec4 murmurHash41(uint src) {
  const uint M = 0x5bd1e995u;
  uvec4 h      = uvec4(1190494759u, 2147483647u, 3559788179u, 179424673u);
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 4 outputs, 1 input
vec4 hash41(float src) {
  uvec4 h = murmurHash41(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec4 murmurHash42(uvec2 src) {
  const uint M = 0x5bd1e995u;
  uvec4 h      = uvec4(1190494759u, 2147483647u, 3559788179u, 179424673u);
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src.x;
  h *= M;
  h ^= src.y;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 4 outputs, 2 inputs
vec4 hash42(vec2 src) {
  uvec4 h = murmurHash42(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec4 murmurHash43(uvec3 src) {
  const uint M = 0x5bd1e995u;
  uvec4 h      = uvec4(1190494759u, 2147483647u, 3559788179u, 179424673u);
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src.x;
  h *= M;
  h ^= src.y;
  h *= M;
  h ^= src.z;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 4 outputs, 3 inputs
vec4 hash43(vec3 src) {
  uvec4 h = murmurHash43(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec4 murmurHash44(uvec4 src) {
  const uint M = 0x5bd1e995u;
  uvec4 h      = uvec4(1190494759u, 2147483647u, 3559788179u, 179424673u);
  src *= M;
  src ^= src >> 24u;
  src *= M;
  h *= M;
  h ^= src.x;
  h *= M;
  h ^= src.y;
  h *= M;
  h ^= src.z;
  h *= M;
  h ^= src.w;
  h ^= h >> 13u;
  h *= M;
  h ^= h >> 15u;
  return h;
}

// 4 outputs, 4 inputs
vec4 hash44(vec4 src) {
  uvec4 h = murmurHash44(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

///

// said to be the best one of its kind...
// https://nullprogram.com/blog/2018/07/31/
uint wellonsHash(uint x) {
  x ^= x >> 16;
  x *= 0x7feb352dU;
  x ^= x >> 15;
  x *= 0x846ca68bU;
  x ^= x >> 16;
  return x;
}

// 3 outputs, 3 inputs
vec3 hash(vec3 src) {
  uvec3 h = murmurHash33(floatBitsToUint(src));
  return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

#endif // HASH_GLSL
