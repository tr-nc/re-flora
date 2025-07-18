#ifndef STARLIGHT_GLSL
#define STARLIGHT_GLSL

#include "../include/core/color.glsl"
#include "../include/core/definitions.glsl"

// Star Nest by Pablo Roman Andrioli
// License: MIT

const int iterations   = 18;
const float formuparam = 0.53;

const int volsteps   = 20;
const float stepsize = 0.1;

const float zoom  = 0.800;
const float tile  = 0.850;
const float speed = 0.010;

const float brightness = 0.0015;
const float darkmatter = 0.300;
const float distfading = 0.730;
const float saturation = 0.850;

vec3 _star_nest_effect(vec3 view_dir) {
    vec3 dir  = view_dir;
    vec3 from = vec3(1.0, 0.5, 2.0);

    // volumetric rendering
    float s    = 0.1;
    float fade = 1.0;
    vec3 v     = vec3(0.0);

    for (int r = 0; r < volsteps; r++) {
        vec3 p = from + s * dir * 0.5;

        // tiling fold
        p = abs(vec3(tile) - mod(p, vec3(tile * 2.0)));

        float pa = 0.0;
        float a  = 0.0;
        for (int i = 0; i < iterations; i++) {
            // the magic formula
            p = abs(p) / dot(p, p) - formuparam;
            // absolute sum of average change
            a += abs(length(p) - pa);
            pa = length(p);
        }

        // dark matter
        float dm = max(0.0, darkmatter - a * a * 0.001);
        // add contrast
        a *= a * a;

        // dark matter, don't render near
        if (r > 6) fade *= 1.0 - dm;

        v += fade;
        // coloring based on distance
        v += vec3(s, s * s, s * s * s * s) * a * brightness * fade;
        // distance fading
        fade *= distfading;
        s += stepsize;
    }

    v = mix(vec3(length(v)), v, saturation); // color adjust
    return v * 0.01;
}

vec3 get_starlight_color(vec3 view_dir) {
    float time_factor = float(env_info.frame_serial_idx) * 0.016;

    vec3 star_color = _star_nest_effect(view_dir);

    float horizon_fade = smoothstep(-0.05, 0.25, view_dir.y);
    star_color *= horizon_fade;

    return srgb_to_linear(star_color);
}

#endif // STARLIGHT_GLSL
