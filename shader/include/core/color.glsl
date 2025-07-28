#ifndef COLOR_GLSL
#define COLOR_GLSL

float luminance(vec3 col) { return dot(col, vec3(0.2126, 0.7152, 0.0722)); }

// the following two functions are taken from:
// https://gamedev.stackexchange.com/questions/92015/optimized-linear-to-srgb-glsl
// when converting vec4s, the alpha channel should be kept as-is
vec3 linear_to_srgb(vec3 c) {
    bvec3 cutoff = lessThan(c, vec3(0.0031308));
    vec3 higher  = vec3(1.055) * pow(c, vec3(1.0 / 2.4)) - vec3(0.055);
    vec3 lower   = c * vec3(12.92);
    return mix(higher, lower, cutoff);
}

vec3 srgb_to_linear(vec3 c) {
    bvec3 cutoff = lessThan(c, vec3(0.04045));
    vec3 higher  = pow((c + vec3(0.055)) / vec3(1.055), vec3(2.4));
    vec3 lower   = c / vec3(12.92);
    return mix(higher, lower, cutoff);
}

// rgb ↔ hsv helpers (all channels 0-1)
// reference: Foley & van Dam
// use linear color space for rgb
vec3 rgb_to_hsv(vec3 c) {
    vec4 K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    vec4 p = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
    vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));

    float d = q.x - min(q.w, q.y);
    float e = 1.0e-10;
    return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

// use linear color space for rgb
vec3 hsv_to_rgb(vec3 c) {
    vec3 p = abs(fract(c.xxx + vec3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);
    return c.z * mix(vec3(1.0), clamp(p - 1.0, 0.0, 1.0), c.y);
}

#endif // COLOR_GLSL
