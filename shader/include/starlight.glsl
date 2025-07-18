#ifndef STARLIGHT_GLSL
#define STARLIGHT_GLSL

#include "../include/core/color.glsl"
#include "../include/core/definitions.glsl"

// Star Nest by Pablo Roman Andrioli
// https://www.shadertoy.com/view/XlfGRj
// License: MIT

struct StarlightInfo {
    int iterations;
    float formuparam;
    int volsteps;
    float stepsize;
    float zoom;
    float tile;
    float speed;
    float brightness;
    float darkmatter;
    float distfading;
    float saturation;
};

vec3 _star_nest_effect(vec3 view_dir, StarlightInfo info) {
    vec3 dir  = view_dir;
    vec3 from = vec3(1.0, 0.5, 2.0);

    // volumetric rendering
    float s    = 0.1;
    float fade = 1.0;
    vec3 v     = vec3(0.0);

    for (int r = 0; r < info.volsteps; r++) {
        vec3 p = from + s * dir * 0.5;

        // tiling fold
        p = abs(vec3(info.tile) - mod(p, vec3(info.tile * 2.0)));

        float pa = 0.0;
        float a  = 0.0;
        for (int i = 0; i < info.iterations; i++) {
            // the magic formula
            p = abs(p) / (dot(p, p) + 1e-6) - info.formuparam;

            float delta_a = abs(length(p) - pa);
            a += min(delta_a, 10.0);
            pa = length(p);
        }

        // dark matter
        float dm = max(0.0, info.darkmatter - a * a * 0.001);
        // add contrast
        a *= a * a;

        // dark matter, don't render near
        if (r > 6) fade *= 1.0 - dm;

        v += fade;
        // coloring based on distance
        v += vec3(s, s * s, s * s * s * s) * a * info.brightness * fade;
        // distance fading
        fade *= info.distfading;
        s += info.stepsize;
    }

    v = mix(vec3(length(v)), v, info.saturation); // color adjust
    return v * 0.01;
}

vec3 get_starlight_color(vec3 view_dir, StarlightInfo info) {
    vec3 star_color = _star_nest_effect(view_dir, info);
    return srgb_to_linear(star_color);
}

#endif // STARLIGHT_GLSL
