#ifndef HASH_GLSL
#define HASH_GLSL

// taken from: https://stackoverflow.com/questions/4200224/random-noise-functions-for-glsl
// Construct a float with half-open range [0:1] using low 23 bits.
// All zeroes yields 0.0, all ones yields the next smallest representable value below 1.0.
float construct_float_01(uint m) {
    const uint ieee_mantissa = 0x007FFFFFu; // binary32 mantissa bitmask
    const uint ieee_one      = 0x3F800000u; // 1.0 in IEEE binary32

    m &= ieee_mantissa; // Keep only mantissa bits (fractional part)
    m |= ieee_one;      // Add fractional part to 1.0

    float f = uintBitsToFloat(m); // Range [1:2]
    return f - 1.0;               // Range [0:1]
}

// Hash Functions
// taken from: https://www.shadertoy.com/view/ttc3zr
//
// murmurHashNM() takes M unsigned integers and returns N hash values.
// The returned values are unsigned integers between 0 and 2^32 - 1.
//
// hashNM() takes M floating point numbers and returns N hash values.
// The returned values are floating point numbers between 0.0 and 1.0.

//------------------------------------------------------------------------------

uint murmur_hash_11(uint src) {
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
    uint h = murmur_hash_11(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uint murmurhash_12(uvec2 src) {
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
float hash_12(vec2 src) {
    uint h = murmurhash_12(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uint murmur_hash_13(uvec3 src) {
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
    uint h = murmur_hash_13(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uint murmur_hash_14(uvec4 src) {
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
float hash_14(vec4 src) {
    uint h = murmur_hash_14(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec2 murmur_hash_21(uint src) {
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
vec2 hash_21(float src) {
    uvec2 h = murmur_hash_21(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec2 murmur_hash_22(uvec2 src) {
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
vec2 hash_22(vec2 src) {
    uvec2 h = murmur_hash_22(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec2 murmur_hash_23(uvec3 src) {
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
vec2 hash_23(vec3 src) {
    uvec2 h = murmur_hash_23(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec2 murmur_hash_24(uvec4 src) {
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
vec2 hash_24(vec4 src) {
    uvec2 h = murmur_hash_24(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec3 murmur_hash_31(uint src) {
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
vec3 hash_31(float src) {
    uvec3 h = murmur_hash_31(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec3 murmur_hash_32(uvec2 src) {
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
vec3 hash_32(vec2 src) {
    uvec3 h = murmur_hash_32(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec3 murmur_hash_33(uvec3 src) {
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
vec3 hash_33(vec3 src) {
    uvec3 h = murmur_hash_33(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec3 murmur_hash_34(uvec4 src) {
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
vec3 hash_34(vec4 src) {
    uvec3 h = murmur_hash_34(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec4 murmur_hash_41(uint src) {
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
vec4 hash_41(float src) {
    uvec4 h = murmur_hash_41(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec4 murmur_hash_42(uvec2 src) {
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
vec4 hash_42(vec2 src) {
    uvec4 h = murmur_hash_42(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec4 murmur_hash_43(uvec3 src) {
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
vec4 hash_43(vec3 src) {
    uvec4 h = murmur_hash_43(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

//------------------------------------------------------------------------------

uvec4 murmur_hash_44(uvec4 src) {
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
vec4 hash_44(vec4 src) {
    uvec4 h = murmur_hash_44(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

///

// said to be the best one of its kind...
// https://nullprogram.com/blog/2018/07/31/
uint wellons_hash(uint x) {
    x ^= x >> 16;
    x *= 0x7feb352dU;
    x ^= x >> 15;
    x *= 0x846ca68bU;
    x ^= x >> 16;
    return x;
}

// 3 outputs, 3 inputs
vec3 hash(vec3 src) {
    uvec3 h = murmur_hash_33(floatBitsToUint(src));
    return uintBitsToFloat(h & 0x007fffffu | 0x3f800000u) - 1.0;
}

#endif // HASH_GLSL
