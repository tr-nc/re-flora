#ifndef GRASS_BAND_COLOR_GLSL
#define GRASS_BAND_COLOR_GLSL

#include "../include/core/color.glsl"
#include "../include/core/gradient_noise.glsl"

float sample_grass_band_noise(vec2 world_xz) {
    // Perlin gradient noise FBM (no large const tables — Metal-safe)
    float noise = fbm_cnoise_2d(world_xz.x, world_xz.y, 9041u, 0.008, 3, 2.0, 0.5);
    return noise * 0.5f + 0.5f;
}

float sample_grass_interpolation_t(float noise_01) { return clamp(noise_01, 0.0f, 1.0f); }

float sample_grass_band_interpolation_t(vec2 world_xz) {
    return sample_grass_interpolation_t(sample_grass_band_noise(world_xz));
}

#endif // GRASS_BAND_COLOR_GLSL
