#ifndef DITHER_GLSL
#define DITHER_GLSL

float bayer_2(vec2 a) {
    a = floor(a);
    return fract(a.x / 2. + a.y * a.y * .75);
}

#define bayer_4(a) (bayer_2(.5 * (a)) * .25 + bayer_2(a))
#define bayer_8(a) (bayer_4(.5 * (a)) * .25 + bayer_2(a))
#define bayer_16(a) (bayer_8(.5 * (a)) * .25 + bayer_2(a))
#define bayer_32(a) (bayer_16(.5 * (a)) * .25 + bayer_2(a))
#define bayer_64(a) (bayer_32(.5 * (a)) * .25 + bayer_2(a))

// taken from: https://www.shadertoy.com/view/MslGR8
vec3 get_dither_mask(ivec2 screen_space_uv) {
    // bit-depth of display. Normally 8 but some LCD monitors are 7 or even 6-bit.
    float dither_bit = 8.0;
    // calculate grid position
    float grid_position =
        fract(dot(vec2(screen_space_uv) - vec2(0.5, 0.5), vec2(1.0 / 16.0, 10.0 / 36.0) + 0.25));

    // calculate how big the shift should be
    float dither_shift = (0.25) * (1.0 / (pow(2.0, dither_bit) - 1.0));

    // shift the individual colors differently, thus making it even harder to see
    // the dithering pattern
    vec3 dither_shift_RGB = vec3(dither_shift, -dither_shift, dither_shift); // subpixel dithering

    // modify shift acording to grid position
    dither_shift_RGB = mix(2.0 * dither_shift_RGB, -2.0 * dither_shift_RGB, grid_position);

    // shift the color by dither_shift
    return 0.5 / 255.0 + dither_shift_RGB;
}

#endif // DITHER_GLSL
