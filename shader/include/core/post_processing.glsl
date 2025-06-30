#ifndef POST_PROCESSING_GLSL
#define POST_PROCESSING_GLSL

#include "./color.glsl"

// tunemap operators
// taken from: https://64.github.io/tonemapping
// combined jodie-reinhard with exteneded reinhard (luminance tune map) for auto exposure
vec3 jodieReinhardTmo(vec3 c, float explosure) {
    vec3 a = c / (lum(c) + 1.0);

    if (explosure <= 1.0) {
        vec3 b = c / explosure;
        return b;
    }

    vec3 b = c * (1.0 + (c / (explosure * explosure))) / (c + 1.0);

    float mixFac = smoothstep(1.0, 1.1, explosure);
    return mix(b, mix(a, b, b), mixFac);
}

#endif // POST_PROCESSING_GLSL
