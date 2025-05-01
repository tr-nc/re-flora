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

#endif // COLOR_GLSL
