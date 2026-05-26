#ifndef WIND_VOLUME_SAMPLE_GLSL
#define WIND_VOLUME_SAMPLE_GLSL

#include "../include/core/definitions.glsl"
#include "../include/core/gradient_noise.glsl"

const uint WIND_GUST_MASK_SEED = 3181u;

const float WIND_SAMPLE_SCALE        = 256.0f;
const float WIND_TIME_SCALE          = 170.0f;
const float WIND_GUST_MASK_FREQUENCY = 0.008f;
const float WIND_GUST_SMOOTH_WIDTH   = 0.30f;
const uint MAX_WIND_SOURCES          = 4u;

float wind_safe_smoothstep(float edge0, float edge1, float x) {
    float low  = min(edge0, edge1);
    float high = max(edge0, edge1);
    if (high - low <= EPSILON) {
        return x >= high ? 1.0f : 0.0f;
    }
    return smoothstep(low, high, x);
}

int wind_safe_octaves(float octaves) { return int(clamp(round(octaves), 1.0f, 8.0f)); }

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

vec4 wind_source_params(uint source_index) {
    if (source_index == 1u) {
        return gui_input.wind_source_1;
    }
    if (source_index == 2u) {
        return gui_input.wind_source_2;
    }
    if (source_index == 3u) {
        return gui_input.wind_source_3;
    }
    return gui_input.wind_source_0;
}

vec4 wind_source_noise_params(uint source_index) {
    if (source_index == 1u) {
        return gui_input.wind_source_1_noise;
    }
    if (source_index == 2u) {
        return gui_input.wind_source_2_noise;
    }
    if (source_index == 3u) {
        return gui_input.wind_source_3_noise;
    }
    return gui_input.wind_source_0_noise;
}

vec4 wind_source_detail_params(uint source_index) {
    if (source_index == 1u) {
        return gui_input.wind_source_1_detail;
    }
    if (source_index == 2u) {
        return gui_input.wind_source_2_detail;
    }
    if (source_index == 3u) {
        return gui_input.wind_source_3_detail;
    }
    return gui_input.wind_source_0_detail;
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
    vec2 sample_pos          = world_pos.xz * WIND_SAMPLE_SCALE;
    float scroll_time        = time * WIND_TIME_SCALE;
    uint wind_source_count   = min(gui_input.wind_source_count, MAX_WIND_SOURCES);
    vec2 wind_planar         = vec2(0.0f);

    for (uint source_index = 0u; source_index < MAX_WIND_SOURCES; ++source_index) {
        if (source_index >= wind_source_count) {
            break;
        }

        vec4 source            = wind_source_params(source_index);
        vec4 noise_params      = wind_source_noise_params(source_index);
        vec4 detail_params     = wind_source_detail_params(source_index);
        float direction_angle  = radians(source.x);
        float source_speed     = max(source.y, 0.0f);
        float source_sharpness = clamp(source.z, 0.0f, 1.0f);
        float source_strength  = max(source.w, 0.0f);
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
