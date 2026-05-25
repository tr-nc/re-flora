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

float wind_safe_smoothstep(float edge0, float edge1, float x) {
    float low  = min(edge0, edge1);
    float high = max(edge0, edge1);
    if (high - low <= EPSILON) {
        return x >= high ? 1.0f : 0.0f;
    }
    return smoothstep(low, high, x);
}

int wind_safe_octaves(uint octaves) { return int(clamp(octaves, 1u, 8u)); }

vec2 wind_safe_normalize(vec2 v, vec2 fallback) {
    float len_sq = dot(v, v);
    if (len_sq <= EPSILON) {
        return fallback;
    }
    return v * inversesqrt(len_sq);
}

vec2 wind_gust_layer_scroll(uint layer) {
    if (layer == 1u) {
        return vec2(-0.72f, 0.61f);
    }
    if (layer == 2u) {
        return vec2(0.23f, 0.97f);
    }
    if (layer == 3u) {
        return vec2(-0.93f, -0.18f);
    }
    return vec2(0.85f, -0.52f);
}

vec2 wind_gust_mask_offset(uint layer) {
    if (layer == 1u) {
        return vec2(-211.0f, 307.0f);
    }
    if (layer == 2u) {
        return vec2(421.0f, 83.0f);
    }
    if (layer == 3u) {
        return vec2(-97.0f, -449.0f);
    }
    return vec2(149.0f, -67.0f);
}

float wind_gust_layer_frequency_scale(uint layer) {
    if (layer == 1u) {
        return 0.82f;
    }
    if (layer == 2u) {
        return 1.18f;
    }
    if (layer == 3u) {
        return 1.43f;
    }
    return 1.0f;
}

float wind_gust_layer_strength_scale(uint layer) {
    if (layer == 1u) {
        return 0.78f;
    }
    if (layer == 2u) {
        return 0.58f;
    }
    if (layer == 3u) {
        return 0.43f;
    }
    return 1.0f;
}

float sample_gust_factor(vec2 sample_pos, vec2 time_offset, uint layer, float sharpness) {
    float frequency = WIND_GUST_MASK_FREQUENCY * wind_gust_layer_frequency_scale(layer);
    vec2 offset     = wind_gust_mask_offset(layer);
    float gust_noise = fbm_cnoise_2d(
        sample_pos.x + offset.x + time_offset.x, sample_pos.y + offset.y + time_offset.y,
        WIND_GUST_MASK_SEED + layer * 997u, frequency, WIND_GUST_MASK_OCTAVES,
        WIND_GUST_LACUNARITY, WIND_GUST_GAIN);

    float gust_value = clamp(gust_noise * 0.5f + 0.5f, 0.0f, 1.0f);
    float center     = (WIND_GUST_SMOOTH_MIN + WIND_GUST_SMOOTH_MAX) * 0.5f;
    float half_width = (WIND_GUST_SMOOTH_MAX - WIND_GUST_SMOOTH_MIN) * 0.5f *
                       (1.0f - clamp(sharpness, 0.0f, 1.0f));
    return wind_safe_smoothstep(center - half_width, center + half_width, gust_value);
}

vec3 sample_procedural_wind(vec3 world_pos, float time) {
    vec2 sample_pos     = world_pos.xz * WIND_SAMPLE_SCALE;
    float scroll_time   = time * WIND_TIME_SCALE;
    uint wind_layers    = clamp(gui_input.wind_layers, 1u, 4u);
    float wind_speed    = max(gui_input.wind_speed, 0.0f);
    float wind_sharpness = clamp(gui_input.wind_sharpness, 0.0f, 1.0f);
    float wind_strength = max(gui_input.wind_strength, 0.0f);
    vec2 wind_planar    = vec2(0.0f);

    if (wind_strength > EPSILON) {
        for (uint layer = 0u; layer < 4u; ++layer) {
            if (layer >= wind_layers) {
                break;
            }

            vec2 layer_scroll   = wind_gust_layer_scroll(layer);
            vec2 wind_direction = wind_safe_normalize(layer_scroll, vec2(1.0f, 0.0f));
            vec2 layer_time     = -wind_direction * scroll_time * wind_speed;
            float wind_factor   = sample_gust_factor(sample_pos, layer_time, layer, wind_sharpness);
            float layer_strength = wind_strength * wind_gust_layer_strength_scale(layer);
            wind_planar += wind_direction * (wind_factor * layer_strength);
        }

        float wind_length = length(wind_planar);
        if (wind_length > wind_strength) {
            wind_planar *= wind_strength / wind_length;
        }
    }

    return vec3(wind_planar.x, 0.0f, wind_planar.y);
}

#endif // WIND_VOLUME_SAMPLE_GLSL
