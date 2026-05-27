#ifndef WIND_VOLUME_SAMPLE_GLSL
#define WIND_VOLUME_SAMPLE_GLSL

#include "../include/core/definitions.glsl"
#include "../include/core/gradient_noise.glsl"

const uint WIND_FBM_SEED = 3181u;
const float WIND_SAMPLE_SCALE = 256.0f;
const float WIND_TIME_SCALE = 170.0f;
const float WIND_FBM_FREQUENCY = 0.008f;
struct WindSourceGpu {
    vec4 params; // direction degrees, speed, gain, unused
    vec4 noise;  // pattern scale, octaves, lacunarity, persistence
};

layout(set = 0, binding = 3) readonly buffer B_WindSources { WindSourceGpu data[]; }
wind_sources;

vec2 wind_source_offset(uint source_index) {
    uint x = source_index * 1664525u + 1013904223u;
    uint y = source_index * 22695477u + 1109515789u;
    return vec2(float(x & 1023u) - 512.0f, float(y & 1023u) - 512.0f);
}

int wind_safe_octaves(float octaves_value) {
    return int(clamp(round(octaves_value), 1.0f, 8.0f));
}

float sample_wind_source_fbm(
    vec2 sample_pos,
    vec2 time_offset,
    uint source_index,
    float gain,
    vec4 noise_params
) {
    float pattern_scale     = max(noise_params.x, 0.05f);
    int octaves             = wind_safe_octaves(noise_params.y);
    float lacunarity        = max(noise_params.z, 1.0f);
    float persistence       = clamp(noise_params.w, 0.0f, 1.0f);
    vec2 offset             = wind_source_offset(source_index);
    vec2 p                  = (sample_pos + offset + time_offset) / pattern_scale;
    float frequency         = WIND_FBM_FREQUENCY;
    float amplitude         = 1.0f;
    float noise_sum         = 0.0f;

    for (int octave = 0; octave < 8; ++octave) {
        if (octave >= octaves) {
            break;
        }
        noise_sum += cnoise_seeded(vec2(p.x, p.y) * frequency, WIND_FBM_SEED + source_index * 997u + uint(octave) * 1000u) * amplitude;
        frequency *= lacunarity;
        amplitude *= persistence;
    }

    return noise_sum * max(gain, 0.0f);
}

vec3 sample_procedural_wind(vec3 world_pos, float time) {
    vec2 sample_pos        = world_pos.xz * WIND_SAMPLE_SCALE;
    float scroll_time      = time * WIND_TIME_SCALE;
    vec2 wind_planar       = vec2(0.0f);

    for (uint source_index = 0u; source_index < gui_input.wind_source_count; ++source_index) {
        WindSourceGpu source_gpu = wind_sources.data[source_index];
        vec4 source              = source_gpu.params;
        vec4 noise_params        = source_gpu.noise;
        float direction_angle    = radians(source.x);
        float source_speed       = max(source.y, 0.0f);
        float source_gain        = max(source.z, 0.0f);
        if (source_gain <= EPSILON) {
            continue;
        }

        vec2 wind_direction = vec2(cos(direction_angle), sin(direction_angle));
        vec2 layer_time     = -wind_direction * scroll_time * source_speed;
        float wind_factor   = sample_wind_source_fbm(
            sample_pos,
            layer_time,
            source_index,
            source_gain,
            noise_params);
        wind_planar += wind_direction * wind_factor;
    }

    return vec3(wind_planar.x, 0.0f, wind_planar.y);
}

#endif // WIND_VOLUME_SAMPLE_GLSL
