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

#endif // DITHER_GLSL
