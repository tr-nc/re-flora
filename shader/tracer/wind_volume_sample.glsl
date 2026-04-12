#ifndef WIND_VOLUME_SAMPLE_GLSL
#define WIND_VOLUME_SAMPLE_GLSL

#include "../include/core/definitions.glsl"
#include "../include/core/gradient_noise.glsl"

const float WIND_DIRECTION_FREQUENCY       = 0.0025f;
const float WIND_STRENGTH_FREQUENCY        = 0.00125f;
const float WIND_MIN_STRENGTH              = 0.5f;
const float WIND_MAX_STRENGTH              = 5.0f;
const float WIND_SAMPLE_SCALE              = 256.0f;
const vec2 WIND_SECOND_SAMPLE_OFFSET       = vec2(57.23f, -113.87f);
const vec2 WIND_STRENGTH_OFFSET            = vec2(-211.0f, 83.0f);
const vec2 WIND_GUST_OFFSET                = vec2(149.0f, -67.0f);
const vec2 WIND_DIRECTION_TIME_SCROLL      = vec2(0.2f, -0.3f);
const vec2 WIND_STRENGTH_TIME_SCROLL       = vec2(-0.3f, 0.5f);
const vec2 WIND_GUST_TIME_SCROLL           = vec2(0.7f, -0.45f);
const float WIND_TIME_SCALE                = 200.0f;
const float WIND_DIRECTION_DETAIL_STRENGTH = 0.5f;
const float WIND_GUST_FREQUENCY            = 0.0035f;
const float WIND_GUST_BOOST                = 0.3f;

vec3 sample_procedural_wind(vec3 world_pos, float time) {
    vec2 sample_pos     = world_pos.xz * WIND_SAMPLE_SCALE;
    float scroll_time   = time * WIND_TIME_SCALE;
    vec2 direction_time = WIND_DIRECTION_TIME_SCROLL * scroll_time;
    vec2 strength_time  = WIND_STRENGTH_TIME_SCROLL * scroll_time;
    vec2 gust_time      = WIND_GUST_TIME_SCROLL * scroll_time;

    float primary_direction_noise =
        fbm_cnoise_2d(sample_pos.x + direction_time.x, sample_pos.y + direction_time.y, 1729u,
                      WIND_DIRECTION_FREQUENCY, 4, 2.0, 0.5);

    float detail_direction_noise =
        fbm_cnoise_2d(sample_pos.x + WIND_SECOND_SAMPLE_OFFSET.x + direction_time.x,
                      sample_pos.y + WIND_SECOND_SAMPLE_OFFSET.y + direction_time.y, 1729u,
                      WIND_DIRECTION_FREQUENCY, 4, 2.0, 0.5);

    float base_angle   = (primary_direction_noise * 0.5f + 0.5f) * TWO_PI;
    float detail_angle = detail_direction_noise * WIND_DIRECTION_DETAIL_STRENGTH;
    vec2 direction     = vec2(cos(base_angle + detail_angle), sin(base_angle + detail_angle));

    float strength_noise = fbm_cnoise_2d(sample_pos.x + WIND_STRENGTH_OFFSET.x + strength_time.x,
                                         sample_pos.y + WIND_STRENGTH_OFFSET.y + strength_time.y,
                                         2843u, WIND_STRENGTH_FREQUENCY, 4, 2.0, 0.5);

    float gust_noise = fbm_cnoise_2d(sample_pos.x + WIND_GUST_OFFSET.x + gust_time.x,
                                     sample_pos.y + WIND_GUST_OFFSET.y + gust_time.y, 3181u,
                                     WIND_GUST_FREQUENCY, 3, 2.0, 0.5);

    float normalized_strength = smoothstep(0.08f, 0.92f, strength_noise * 0.5f + 0.5f);
    float gust_factor         = smoothstep(0.2f, 0.85f, gust_noise * 0.5f + 0.5f);
    float strength_mix        = clamp(normalized_strength + gust_factor * WIND_GUST_BOOST,
                                      0.0f, 1.0f);
    float strength            = mix(WIND_MIN_STRENGTH, WIND_MAX_STRENGTH, strength_mix);
    vec2 wind_planar = direction * strength;
    return vec3(wind_planar.x, 0.0f, wind_planar.y);
}

#endif // WIND_VOLUME_SAMPLE_GLSL
