#ifndef SKYLIGHT_GLSL
#define SKYLIGHT_GLSL

#include "../include/core/color.glsl"

vec3 get_sky_color(vec3 dir, vec3 sky_color) { 
    return srgb_to_linear(sky_color); 
}

vec3 get_sky_color_with_sun(vec3 view_dir, vec3 sun_dir, vec3 sun_color, float sun_luminance, float sun_size, vec3 sky_color) {
    vec3 sky_color_linear = get_sky_color(view_dir, sky_color);

    vec3 luminance_sun_color = sun_color * sun_luminance;
    float sun_intensity   = max(0.0, dot(view_dir, sun_dir));
    float sun_power       = max(1.0, 100.0 / max(0.01, sun_size * 10.0));
    sun_intensity         = pow(sun_intensity, sun_power);

    return mix(sky_color_linear, luminance_sun_color, sun_intensity);
}

#endif // SKYLIGHT_GLSL
