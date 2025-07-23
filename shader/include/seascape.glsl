
#ifndef SEASCAPE_GLSL
#define SEASCAPE_GLSL

/*
 * taken from: https://www.shadertoy.com/view/Ms2SD1 for the _noise functions
 * taken from: https://www.shadertoy.com/view/MdXyzX for the marching functions
 * "Seascape" by Alexander Alekseev aka TDM - 2014
 * License Creative Commons Attribution-NonCommercial-ShareAlike 3.0 Unported License.
 * Contact: tdmaav@gmail.com
 */

#include "../include/core/definitions.glsl"
#include "../include/core/hash.glsl"

const int SEASCAPE_NUM_STEPS = 8;
const float SEASCAPE_EPSILON = 1e-3;

const float WATER_TOP_HEIGHT    = 0.05;
const float WATER_BOTTOM_HEIGHT = 0.01;

const int SEASCAPE_ITER_RAYMARCH = 3;
const int SEASCAPE_ITER_NORMAL   = 5;
const float SEA_CHOPPY           = 4.0;
const float SEA_FREQ             = 0.6;
#define SEA_TIME (1.0 + renderInfoUbo.data.time * 0.2)
const mat2 OCTAVE_TRANSFORM = mat2(1.6, 1.2, -1.2, 1.6);

// -1.0 - 1.0
float _seascape_noise(in vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return 2.0 * mix(mix(hash12(i + vec2(0.0, 0.0)), hash12(i + vec2(1.0, 0.0)), u.x),
                     mix(hash12(i + vec2(0.0, 1.0)), hash12(i + vec2(1.0, 1.0)), u.x), u.y) -
           1.0;
}

float _seascape_octave(vec2 uv, float choppy) {
    uv += _seascape_noise(uv);
    vec2 wv  = 1.0 - abs(sin(uv));
    vec2 swv = abs(cos(uv));
    wv       = mix(wv, swv, wv);
    return pow(1.0 - pow(wv.x * wv.y, 0.65), choppy);
}

// 0.0 - 1.0
float _get_wave_height_01(vec2 p, uint iteration_count) {
    float freq   = SEA_FREQ;
    float amp    = 1.0;
    float choppy = SEA_CHOPPY;
    vec2 uv      = p;
    uv.x *= 0.75;

    float d, h = 0.0;
    float sum_of_amp = 0.0;
    for (uint i = 0; i < iteration_count; i++) {
        // 0.0 - 1.0
        d = _seascape_octave((uv + SEA_TIME) * freq, choppy);
        d += _seascape_octave((uv - SEA_TIME) * freq, choppy);
        d /= 2.0;
        sum_of_amp += amp;
        h += d * amp;
        uv *= OCTAVE_TRANSFORM;
        freq *= 1.9;
        amp *= 0.22;
        choppy = mix(choppy, 1.0, 0.2);
    }
    h /= sum_of_amp;
    return h;
}

// ray-plane intersection checker
float _intersect_plane(vec3 origin, vec3 direction, vec3 point, vec3 normal) {
    return clamp(dot(point - origin, normal) / dot(direction, normal), -1.0, 9991999.0);
}

// assertion: water must be hit before calling this function
// raymarches the ray from top water layer boundary to low water layer boundary
float _raymarch_water(vec3 o, vec3 d) {
    // calculate intersections and reconstruct positions
    float t_high_plane =
        _intersect_plane(o, d, vec3(0.0, WATER_TOP_HEIGHT, 0.0), vec3(0.0, 1.0, 0.0));
    float t_low_plane =
        _intersect_plane(o, d, vec3(0.0, WATER_BOTTOM_HEIGHT, 0.0), vec3(0.0, 1.0, 0.0));
    vec3 start = o + t_high_plane * d; // high hit position
    vec3 end   = o + t_low_plane * d;  // low hit position

    float t  = t_high_plane;
    vec3 pos = start;

    for (int i = 0; i < 64; i++) {
        float h = mix(WATER_BOTTOM_HEIGHT, WATER_TOP_HEIGHT,
                      _get_wave_height_01(pos.xz, SEASCAPE_ITER_RAYMARCH));

        if (h + 1e-3 * t > pos.y) {
            return t;
        }
        t += pos.y - h;
        // iterate forwards according to the height mismatch
        pos = o + t * d;
    }
    // if hit was not registered, just assume hit the top layer,
    // this makes the raymarching faster and looks better at higher distances
    return t_high_plane;
}

// calculate normal at point by calculating the height at the pos and 2 additional points very
// close to pos
vec3 _calculate_normal(vec2 pos, float eps) {
    vec2 ex     = vec2(eps, 0);
    float depth = WATER_TOP_HEIGHT - WATER_BOTTOM_HEIGHT;
    float h     = _get_wave_height_01(pos, SEASCAPE_ITER_NORMAL) * depth;
    vec3 a      = vec3(pos.x, h, pos.y);
    return normalize(
        cross(a - vec3(pos.x - eps, _get_wave_height_01(pos - ex.xy, SEASCAPE_ITER_NORMAL) * depth,
                       pos.y),
              a - vec3(pos.x, _get_wave_height_01(pos + ex.yx, SEASCAPE_ITER_NORMAL) * depth,
                       pos.y + eps)));
}

bool trace_seascape(out vec3 o_position, out vec3 o_normal, out float o_t, vec3 o, vec3 d) {
    o_t        = 1e10;
    o_position = o + d * o_t;

    if (d.y >= 0.0) {
        return false;
    }

    // raymatch water and reconstruct the hit pos
    float dist         = _raymarch_water(o, d);
    vec3 water_hit_pos = o + d * dist;

    vec3 norm = _calculate_normal(water_hit_pos.xz, 0.01);

    o_t        = dist;
    o_position = water_hit_pos;

    // smooth the normal with distance to avoid disturbing high frequency noise
    o_normal = mix(norm, vec3(0.0, 1.0, 0.0), 0.8 * min(1.0, sqrt(dist * 0.01) * 1.1));

    return true;
}

#endif // SEASCAPE_GLSL
