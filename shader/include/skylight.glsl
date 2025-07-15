#ifndef SKYLIGHT_GLSL
#define SKYLIGHT_GLSL

vec3 get_sky_color(vec3 dir) { return vec3(0.5, 0.7, 1.0); }

vec3 get_sky_color_with_sun(vec3 view_dir, vec3 sun_dir, vec3 sun_color) {
    vec3 sky_color = vec3(0.5, 0.7, 1.0);
    float sun_intensity = max(0.0, dot(view_dir, sun_dir));
    sun_intensity = pow(sun_intensity, 100.0); // Sharp falloff for sun disk
    return mix(sky_color, sun_color, sun_intensity);
}

#endif // SKYLIGHT_GLSL
