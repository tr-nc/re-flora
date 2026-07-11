#ifndef FLORA_PALETTE_GLSL
#define FLORA_PALETTE_GLSL

#include "../include/core/hash.glsl"
#include "../include/flora_registry.glsl"

// Lavender uses explicit per-height rows instead of a green-to-flower gradient.
// Rows 0..6 map to the stem/sepals and stay at the original green
// [sRGB 74,165,0]. Rows 7..11 map to the flower ball and use palette-specific
// petal color layers. Colors are stored in linear RGB10.

const uint LAVENDER_PALETTE_LEN = 6u;
const int LAVENDER_HEIGHT_PALETTE_RGB10_LEN = 72;
const uint LAVENDER_HEIGHT_PALETTE_RGB10[LAVENDER_HEIGHT_PALETTE_RGB10_LEN] = uint[](
    // Golden Bloom [sRGB 0.984, 0.749, 0.141]
    0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u,
    0x00060446u, 0x00C5AA9Fu, 0x01075764u, 0x012857DAu, 0x01492BFFu, 0x0117EBA9u,
    // Rose Petal [sRGB 0.984, 0.443, 0.522]
    0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u,
    0x00060446u, 0x0A31CE9Fu, 0x0D425764u, 0x0F02A7DAu, 0x1082EBFFu, 0x0E4283A9u,
    // Soft Peony [sRGB 0.984, 0.812, 0.910]
    0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u,
    0x00060446u, 0x2326CA9Fu, 0x2D78CB64u, 0x33A9FFDAu, 0x38DAFFFFu, 0x31197FA9u,
    // Deep Rose [sRGB 0.882, 0.114, 0.282]
    0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u,
    0x00060446u, 0x02D0260Bu, 0x03A02EA5u, 0x04203702u, 0x04903B4Fu, 0x03F032DBu,
    // Sky Azure [sRGB 0.490, 0.827, 0.988]
    0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u,
    0x00060446u, 0x2A57148Fu, 0x36C928B8u, 0x3E3A68D2u, 0x3FFB70E7u, 0x3B29E0C7u,
    // Deep Violet [sRGB 0.545, 0.462, 0.973]
    0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u, 0x00060446u,
    0x00060446u, 0x28E1F8B4u, 0x34E28CE8u, 0x3C12E508u, 0x3FF32D22u, 0x3912BCFBu);

uint combine_color_seed(uint seed) {
    // lightweight scramble to decorrelate neighboring instances
    return wellons_hash(seed);
}

// Uniformly map a seed to a palette bucket in [0, palette_len)
uint sample_palette_bucket(uint seed, uint palette_len) {
    float r = construct_float_01(seed);
    return uint(r * float(palette_len));
}

uint sample_lavender_height_palette_rgb10(uint seed, uint height_row) {
    uint palette_idx = sample_palette_bucket(seed, LAVENDER_PALETTE_LEN);
    uint clamped_row = min(height_row, uint(FLORA_HEIGHT_COLOR_TABLE_LEN - 1));
    return LAVENDER_HEIGHT_PALETTE_RGB10[palette_idx * uint(FLORA_HEIGHT_COLOR_TABLE_LEN) +
                                         clamped_row];
}

#endif // FLORA_PALETTE_GLSL
