#ifndef SKYLIGHT_GLSL
#define SKYLIGHT_GLSL

#include "../include/core/color.glsl"

struct SkyColors {
    vec3 top_color;
    vec3 bottom_color;
};

// Sun altitude ranges from -1 to 1
SkyColors get_sky_color_by_sun_altitude(float sun_altitude) {
    SkyColors result;

    // Define color points for different sun altitudes
    vec3 day_top    = srgb_to_linear(vec3(0.0, 108.0, 206.0) / 255.0);
    vec3 day_bottom = srgb_to_linear(vec3(255.0, 195.0, 92.0) / 255.0);

    vec3 sunset_top    = srgb_to_linear(vec3(59.0, 114.0, 180.0) / 255.0);
    vec3 sunset_bottom = srgb_to_linear(vec3(191.0, 126.0, 145.0) / 255.0);

    vec3 night_top    = srgb_to_linear(vec3(0.0, 28.0, 56.0) / 255.0);
    vec3 night_bottom = srgb_to_linear(vec3(45.0, 43.0, 60.0) / 255.0);

    if (sun_altitude >= 0.4) {
        // Full day
        result.top_color    = day_top;
        result.bottom_color = day_bottom;
    } else if (sun_altitude > -0.1) {
        // Transition from day to sunset
        float t             = (sun_altitude - 0.1) / (0.4 - 0.1);
        result.top_color    = mix(sunset_top, day_top, t);
        result.bottom_color = mix(sunset_bottom, day_bottom, t);
    } else if (sun_altitude > -0.2) {
        // Transition from sunset to night
        float t             = (sun_altitude - (-0.2)) / (0.1 - (-0.2));
        result.top_color    = mix(night_top, sunset_top, t);
        result.bottom_color = mix(night_bottom, sunset_bottom, t);
    } else {
        // Full night
        result.top_color    = night_top;
        result.bottom_color = night_bottom;
    }

    return result;
}

vec3 get_sky_color(vec3 view_dir, vec3 sun_dir) {
    // Altitude range now matches sun altitude range (-1 to 1)
    float altitude     = view_dir.y;
    float sun_altitude = sun_dir.y;

    SkyColors sky_colors = get_sky_color_by_sun_altitude(sun_altitude);

    vec3 sky_color;
    if (altitude < -0.2) {
        sky_color = sky_colors.bottom_color;
    } else if (altitude < 0.4) {
        float transition = (altitude - (-0.2)) / (0.4 - (-0.2));
        transition       = smoothstep(0.0, 1.0, transition);
        sky_color        = mix(sky_colors.bottom_color, sky_colors.top_color, transition);
    } else {
        sky_color = sky_colors.top_color;
    }

    return srgb_to_linear(sky_color);
}

vec3 get_sky_color_with_sun(vec3 view_dir, vec3 sun_dir, vec3 sun_color, float sun_luminance,
                            float sun_size) {
    vec3 sky_color_linear = get_sky_color(view_dir, sun_dir);

    vec3 luminance_sun_color = sun_color * sun_luminance;
    float sun_intensity      = max(0.0, dot(view_dir, sun_dir));
    float sun_power          = max(1.0, 100.0 / max(0.01, sun_size * 10.0));
    sun_intensity            = pow(sun_intensity, sun_power);

    return mix(sky_color_linear, luminance_sun_color, sun_intensity);
}

#endif // SKYLIGHT_GLSL
