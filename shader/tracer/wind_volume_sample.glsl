#ifndef WIND_VOLUME_SAMPLE_GLSL
#define WIND_VOLUME_SAMPLE_GLSL

#include "../include/core/definitions.glsl"
#include "../include/core/gradient_noise.glsl"

float wind_safe_smoothstep(float edge0, float edge1, float x) {
    float low  = min(edge0, edge1);
    float high = max(edge0, edge1);
    if (high - low <= EPSILON) {
        return x >= high ? 1.0f : 0.0f;
    }
    return smoothstep(low, high, x);
}

int wind_safe_octaves(uint octaves) { return int(clamp(octaves, 1u, 8u)); }

vec2 sample_wind_direction(vec2 sample_pos, vec2 primary_offset, vec2 detail_offset,
                           vec2 time_offset, uint seed, float frequency, uint octaves,
                           float lacunarity, float gain, float detail_strength) {
    float safe_frequency  = max(frequency, 0.0f);
    float safe_lacunarity = max(lacunarity, 0.001f);
    float safe_gain       = max(gain, 0.0f);
    int safe_octaves      = wind_safe_octaves(octaves);

    float primary_noise = fbm_cnoise_2d(sample_pos.x + primary_offset.x + time_offset.x,
                                        sample_pos.y + primary_offset.y + time_offset.y, seed,
                                        safe_frequency, safe_octaves, safe_lacunarity, safe_gain);
    float detail_noise  = fbm_cnoise_2d(sample_pos.x + detail_offset.x + time_offset.x,
                                        sample_pos.y + detail_offset.y + time_offset.y, seed,
                                        safe_frequency, safe_octaves, safe_lacunarity, safe_gain);

    float angle = (primary_noise * 0.5f + 0.5f) * TWO_PI + detail_noise * detail_strength;
    return vec2(cos(angle), sin(angle));
}

vec3 sample_procedural_wind(vec3 world_pos, float time) {
    uint wind_mode = min(gui_input.wind_mode, 3u);
    bool use_gust  = wind_mode == 1u || wind_mode == 3u;
    bool use_base  = wind_mode == 2u || wind_mode == 3u;
    if (!use_base && !use_gust) {
        return vec3(0.0f);
    }

    vec2 sample_pos   = world_pos.xz * max(gui_input.wind_sample_scale, 0.0f);
    float scroll_time = time * gui_input.wind_time_scale;

    vec2 base_direction_detail_offset =
        vec2(gui_input.wind_second_sample_offset_x, gui_input.wind_second_sample_offset_y);
    vec2 strength_offset = vec2(gui_input.wind_strength_offset_x, gui_input.wind_strength_offset_y);
    vec2 gust_mask_offset = vec2(gui_input.wind_gust_offset_x, gui_input.wind_gust_offset_y);
    vec2 gust_direction_offset =
        vec2(gui_input.wind_gust_direction_offset_x, gui_input.wind_gust_direction_offset_y);
    vec2 gust_direction_detail_offset =
        gust_direction_offset + vec2(gui_input.wind_gust_direction_second_offset_x,
                                     gui_input.wind_gust_direction_second_offset_y);

    vec2 base_direction_time =
        vec2(gui_input.wind_direction_time_scroll_x, gui_input.wind_direction_time_scroll_y) *
        scroll_time;
    vec2 strength_time =
        vec2(gui_input.wind_strength_time_scroll_x, gui_input.wind_strength_time_scroll_y) *
        scroll_time;
    vec2 gust_mask_time = vec2(gui_input.wind_gust_time_scroll_x,
                               gui_input.wind_gust_time_scroll_y) *
                          scroll_time;
    vec2 gust_direction_time = vec2(gui_input.wind_gust_direction_time_scroll_x,
                                    gui_input.wind_gust_direction_time_scroll_y) *
                               scroll_time;

    vec2 base_direction = sample_wind_direction(
        sample_pos, vec2(0.0f), base_direction_detail_offset, base_direction_time,
        gui_input.wind_direction_seed, gui_input.wind_direction_frequency,
        gui_input.wind_direction_octaves, gui_input.wind_direction_lacunarity,
        gui_input.wind_direction_gain, gui_input.wind_direction_detail_strength);

    vec2 gust_direction = sample_wind_direction(
        sample_pos, gust_direction_offset, gust_direction_detail_offset, gust_direction_time,
        gui_input.wind_gust_direction_seed, gui_input.wind_gust_direction_frequency,
        gui_input.wind_gust_direction_octaves, gui_input.wind_gust_direction_lacunarity,
        gui_input.wind_gust_direction_gain, gui_input.wind_gust_direction_detail_strength);

    float strength_noise = fbm_cnoise_2d(
        sample_pos.x + strength_offset.x + strength_time.x,
        sample_pos.y + strength_offset.y + strength_time.y, gui_input.wind_strength_seed,
        max(gui_input.wind_strength_frequency, 0.0f),
        wind_safe_octaves(gui_input.wind_strength_octaves),
        max(gui_input.wind_strength_lacunarity, 0.001f), max(gui_input.wind_strength_gain, 0.0f));

    float gust_noise = fbm_cnoise_2d(
        sample_pos.x + gust_mask_offset.x + gust_mask_time.x,
        sample_pos.y + gust_mask_offset.y + gust_mask_time.y, gui_input.wind_gust_seed,
        max(gui_input.wind_gust_frequency, 0.0f), wind_safe_octaves(gui_input.wind_gust_octaves),
        max(gui_input.wind_gust_lacunarity, 0.001f), max(gui_input.wind_gust_gain, 0.0f));

    float base_strength_mix = wind_safe_smoothstep(gui_input.wind_strength_smooth_min,
                                                   gui_input.wind_strength_smooth_max,
                                                   strength_noise * 0.5f + 0.5f);
    float gust_factor       = wind_safe_smoothstep(gui_input.wind_gust_smooth_min,
                                                   gui_input.wind_gust_smooth_max,
                                                   gust_noise * 0.5f + 0.5f);

    float base_min_strength = min(gui_input.wind_min_strength, gui_input.wind_max_strength);
    float base_max_strength = max(gui_input.wind_min_strength, gui_input.wind_max_strength);
    float base_strength     = mix(base_min_strength, base_max_strength, base_strength_mix);
    float gust_strength     = gust_factor * max(gui_input.wind_gust_boost, 0.0f);

    vec2 wind_planar = vec2(0.0f);
    if (use_base) {
        wind_planar += base_direction * base_strength;
    }
    if (use_gust) {
        wind_planar += gust_direction * gust_strength;
    }

    if (gui_input.wind_output_max_strength > EPSILON) {
        float wind_length = length(wind_planar);
        if (wind_length > gui_input.wind_output_max_strength) {
            wind_planar *= gui_input.wind_output_max_strength / wind_length;
        }
    }

    return vec3(wind_planar.x, 0.0f, wind_planar.y);
}

#endif // WIND_VOLUME_SAMPLE_GLSL
