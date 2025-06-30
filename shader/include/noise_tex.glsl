/// Noise texture utilities
/// Requires:
/// define: scalar_bn
/// define: unit_vec2_bn
/// define: unit_vec3_bn
/// define: weighted_cosine_bn
/// define: fast_unit_vec3_bn
/// define: fast_weighted_cosine_bn

#ifndef NOISE_TEX_GLSL
#define NOISE_TEX_GLSL

const ivec3 BN_NOISE_TEX_SIZE = ivec3(128, 128, 64);
ivec3 get_seed(uint frame_serial_idx) {
    return ivec3(gl_GlobalInvocationID.xy % 128, frame_serial_idx % 64);
}

// range (0, 1)
float random_float_bn(ivec3 seed) { return imageLoad(scalar_bn, seed).r; }

// range (-1, 1) in 2 components
vec2 random_unit_vec2_bn(ivec3 seed) { return imageLoad(unit_vec2_bn, seed).rg * 2.0 - 1.0; }

// range (-1, 1) in 3 components
vec3 random_unit_vec3_bn(ivec3 seed) { return imageLoad(fast_unit_vec3_bn, seed).rgb * 2.0 - 1.0; }

// range (-1, 1) in x and y, range (0, 1) in z
vec3 random_weighted_cosine_bn(ivec3 seed) {
    return imageLoad(fast_weighted_cosine_bn, seed).rgb * 2.0 - 1.0;
}

#endif // NOISE_TEX_GLSL
