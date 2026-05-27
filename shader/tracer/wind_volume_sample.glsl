#ifndef WIND_VOLUME_SAMPLE_GLSL
#define WIND_VOLUME_SAMPLE_GLSL

#include "../include/core/definitions.glsl"
#include "../include/core/gradient_noise.glsl"

const uint WIND_GUST_MASK_SEED = 3181u;
const float WIND_SAMPLE_SCALE = 256.0f;
const float WIND_TIME_SCALE = 170.0f;
const float WIND_GUST_MASK_FREQUENCY = 0.008f;
const float WIND_GUST_SMOOTH_WIDTH = 0.30f;

struct WindSourceGpu {
    vec4 params; // direction degrees, speed, sharpness, strength
    vec4 noise;  // coverage, pattern scale, pattern frequency, octaves
    vec4 detail; // lacunarity, gain, unused, unused
};

layout(set = 0, binding = 3) readonly buffer B_WindSources { WindSourceGpu data[]; }
wind_sources;

float wind_safe_smoothstep(float edge0, float edge1, float x) {
    float low  = min(edge0, edge1);
    float high = max(edge0, edge1);
    if (high - low <= EPSILON) {
        return x >= high ? 1.0f : 0.0f;
    }

    float t = clamp((x - low) / (high - low), 0.0f, 1.0f);
    return t * t * (3.0f - 2.0f * t);
}

vec2 wind_source_offset(uint source_index) {
    uint x = source_index * 1664525u + 1013904223u;
    uint y = source_index * 22695477u + 1109515789u;
    return vec2(float(x & 1023u) - 512.0f, float(y & 1023u) - 512.0f);
}

int wind_safe_octaves(float octaves_value) {
    return int(clamp(round(octaves_value), 1.0f, 8.0f));
}

float sample_wind_source_gust(
    vec2 sample_pos,
    vec2 time_offset,
    uint source_index,
    float sharpness,
    vec4 noise_params,
    vec4 detail_params
) {
    float coverage          = clamp(noise_params.x, 0.0f, 1.0f);
    float pattern_scale     = max(noise_params.y, 0.05f);
    float pattern_frequency = max(noise_params.z, 0.05f);
    int octaves             = wind_safe_octaves(noise_params.w);
    float lacunarity        = max(detail_params.x, 1.0f);
    float gain              = clamp(detail_params.y, 0.0f, 1.0f);
    vec2 offset             = wind_source_offset(source_index);
    vec2 p                  = (sample_pos + offset + time_offset) / pattern_scale;
    float frequency         = WIND_GUST_MASK_FREQUENCY * pattern_frequency;
    float amplitude         = 1.0f;
    float amplitude_sum     = 0.0f;
    float noise_sum         = 0.0f;

    for (int octave = 0; octave < 8; ++octave) {
        if (octave >= octaves) {
            break;
        }
        noise_sum += cnoise_seeded(vec2(p.x, p.y) * frequency, WIND_GUST_MASK_SEED + source_index * 997u + uint(octave) * 1000u) * amplitude;
        amplitude_sum += amplitude;
        frequency *= lacunarity;
        amplitude *= gain;
    }

    float gust_noise = amplitude_sum <= EPSILON ? 0.0f : noise_sum / amplitude_sum;
    float gust_value = clamp(gust_noise * 0.5f + 0.5f, 0.0f, 1.0f);
    float center     = 1.0f - coverage;
    float half_width = WIND_GUST_SMOOTH_WIDTH * 0.5f * (1.0f - clamp(sharpness, 0.0f, 1.0f));
    return wind_safe_smoothstep(center - half_width, center + half_width, gust_value);
}

vec3 sample_procedural_wind(vec3 world_pos, float time) {
    vec2 sample_pos        = world_pos.xz * WIND_SAMPLE_SCALE;
    float scroll_time      = time * WIND_TIME_SCALE;
    vec2 wind_planar       = vec2(0.0f);

    for (uint source_index = 0u; source_index < gui_input.wind_source_count; ++source_index) {
        WindSourceGpu source_gpu = wind_sources.data[source_index];
        vec4 source              = source_gpu.params;
        vec4 noise_params        = source_gpu.noise;
        vec4 detail_params       = source_gpu.detail;
        float direction_angle    = radians(source.x);
        float source_speed       = max(source.y, 0.0f);
        float source_sharpness   = clamp(source.z, 0.0f, 1.0f);
        float source_strength    = max(source.w, 0.0f);
        if (source_strength <= EPSILON) {
            continue;
        }

        vec2 wind_direction = vec2(cos(direction_angle), sin(direction_angle));
        vec2 layer_time     = -wind_direction * scroll_time * source_speed;
        float wind_factor   = sample_wind_source_gust(
            sample_pos,
            layer_time,
            source_index,
            source_sharpness,
            noise_params,
            detail_params);
        wind_planar += wind_direction * (wind_factor * source_strength);
    }

    return vec3(wind_planar.x, 0.0f, wind_planar.y);
}

#endif // WIND_VOLUME_SAMPLE_GLSL
