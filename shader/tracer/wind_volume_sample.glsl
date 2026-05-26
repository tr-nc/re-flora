#ifndef WIND_VOLUME_SAMPLE_GLSL
#define WIND_VOLUME_SAMPLE_GLSL

#include "../include/core/definitions.glsl"
#include "../include/core/gradient_noise.glsl"

const uint WIND_GUST_MASK_SEED = 3181u;

const float WIND_SAMPLE_SCALE        = 256.0f;
const float WIND_TIME_SCALE          = 170.0f;
const float WIND_GUST_MASK_FREQUENCY = 0.008f;
const int WIND_GUST_MASK_OCTAVES     = 3;
const float WIND_GUST_LACUNARITY     = 2.0f;
const float WIND_GUST_GAIN           = 0.5f;
const float WIND_GUST_SMOOTH_MIN     = 0.52f;
const float WIND_GUST_SMOOTH_MAX     = 0.82f;
const uint MAX_WIND_SOURCES          = 4u;

float wind_safe_smoothstep(float edge0, float edge1, float x) {
    float low  = min(edge0, edge1);
    float high = max(edge0, edge1);
    if (high - low <= EPSILON) {
        return x >= high ? 1.0f : 0.0f;
    }
    return smoothstep(low, high, x);
}

vec2 wind_source_offset(uint source_index) {
    if (source_index == 1u) {
        return vec2(-211.0f, 307.0f);
    }
    if (source_index == 2u) {
        return vec2(421.0f, 83.0f);
    }
    if (source_index == 3u) {
        return vec2(-97.0f, -449.0f);
    }
    return vec2(149.0f, -67.0f);
}

float sample_wind_source_gust(
    vec2 sample_pos,
    vec2 time_offset,
    uint source_index,
    float frequency,
    float sharpness
) {
    float source_frequency = WIND_GUST_MASK_FREQUENCY * max(frequency, 0.0f);
    vec2 offset            = wind_source_offset(source_index);
    float gust_noise       = fbm_cnoise_2d(
        sample_pos.x + offset.x + time_offset.x,
        sample_pos.y + offset.y + time_offset.y,
        WIND_GUST_MASK_SEED + source_index * 997u,
        source_frequency,
        WIND_GUST_MASK_OCTAVES,
        WIND_GUST_LACUNARITY,
        WIND_GUST_GAIN);

    float gust_value = clamp(gust_noise * 0.5f + 0.5f, 0.0f, 1.0f);
    float center     = (WIND_GUST_SMOOTH_MIN + WIND_GUST_SMOOTH_MAX) * 0.5f;
    float half_width = (WIND_GUST_SMOOTH_MAX - WIND_GUST_SMOOTH_MIN) * 0.5f *
                       (1.0f - clamp(sharpness, 0.0f, 1.0f));
    return wind_safe_smoothstep(center - half_width, center + half_width, gust_value);
}

vec3 sample_procedural_wind(vec3 world_pos, float time) {
    vec2 sample_pos          = world_pos.xz * WIND_SAMPLE_SCALE;
    float scroll_time        = time * WIND_TIME_SCALE;
    uint wind_source_count   = min(gui_input.wind_source_count, MAX_WIND_SOURCES);
    vec2 wind_planar         = vec2(0.0f);

    for (uint source_index = 0u; source_index < MAX_WIND_SOURCES; ++source_index) {
        if (source_index >= wind_source_count) {
            break;
        }

        vec4 source = gui_input.wind_source_0;
        if (source_index == 1u) {
            source = gui_input.wind_source_1;
        } else if (source_index == 2u) {
            source = gui_input.wind_source_2;
        } else if (source_index == 3u) {
            source = gui_input.wind_source_3;
        }
        float direction_angle  = radians(source.x);
        float source_frequency = max(source.y, 0.0f);
        float source_sharpness = clamp(source.z, 0.0f, 1.0f);
        float source_strength  = max(source.w, 0.0f);
        if (source_strength <= EPSILON || source_frequency <= EPSILON) {
            continue;
        }

        vec2 wind_direction = vec2(cos(direction_angle), sin(direction_angle));
        vec2 layer_time     = -wind_direction * scroll_time * source_frequency;
        float wind_factor   = sample_wind_source_gust(
            sample_pos,
            layer_time,
            source_index,
            source_frequency,
            source_sharpness);
        wind_planar += wind_direction * (wind_factor * source_strength);
    }

    return vec3(wind_planar.x, 0.0f, wind_planar.y);
}

#endif // WIND_VOLUME_SAMPLE_GLSL
