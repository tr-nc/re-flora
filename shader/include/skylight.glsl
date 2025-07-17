#ifndef SKYLIGHT_GLSL
#define SKYLIGHT_GLSL

#include "../include/core/color.glsl"

struct SkyColors {
    vec3 top_color;
    vec3 bottom_color;
};

// Keyframe structure for time-of-day transitions
struct TimeOfDayKeyframe {
    float sun_altitude;
    vec3 top_color;
    vec3 bottom_color;
};

// Keyframe structure for view altitude transitions
struct ViewAltitudeKeyframe {
    float view_altitude;
    float blend_factor; // 0.0 = bottom color, 1.0 = top color
};

// Time-of-day keyframes - A more vibrant and detailed progression
// We've increased the number of keyframes for smoother and more colorful transitions.
const int TIME_KEYFRAME_COUNT                               = 9;
const TimeOfDayKeyframe TIME_KEYFRAMES[TIME_KEYFRAME_COUNT] = {
    // 1. Deep Night: Almost pitch black with a hint of deep, cold blue.
    {-0.4, vec3(4.0, 5.0, 10.0) / 255.0, vec3(8.0, 10.0, 20.0) / 255.0},

    // 2. Pre-Dawn: The first light appears as a subtle purple/magenta glow on the horizon.
    {-0.18, vec3(20.0, 25.0, 45.0) / 255.0, vec3(40.0, 30.0, 50.0) / 255.0},

    // 3. Dawn/Dusk Glow: The sky above is a deep blue, while the horizon burns with a vibrant red
    // glow.
    {-0.05, vec3(30.0, 50.0, 100.0) / 255.0, vec3(180.0, 50.0, 90.0) / 255.0},

    // 4. Sunrise/Sunset: The sun is at the horizon. The top of the sky is a lighter blue, meeting a
    // fiery orange.
    // This is the key to the "blue top, colorful bottom" sunset.
    {0.0, vec3(80.0, 100.0, 180.0) / 255.0, vec3(255.0, 100.0, 40.0) / 255.0},

    // 5. Golden Hour: As the sun rises, the fiery reds give way to a warm, golden light.
    {0.05, vec3(135.0, 170.0, 255.0) / 255.0, vec3(255.0, 180.0, 100.0) / 255.0},

    // 6. Morning: The sky clears to a crisp, welcoming blue. A pale cyan near the horizon suggests
    // morning haze.
    {0.2, vec3(100.0, 160.0, 255.0) / 255.0, vec3(180.0, 210.0, 255.0) / 255.0},

    // 7. Mid-day: A classic, bright sky blue. The variation between top and bottom adds depth.
    {0.5, vec3(80.0, 150.0, 255.0) / 255.0, vec3(110.0, 170.0, 245.0) / 255.0},

    // 8. High Noon: At its peak, the sun makes the zenith a deep, pure blue, creating a subtle
    // daytime gradient.
    {0.8, vec3(60.0, 120.0, 230.0) / 255.0, vec3(90.0, 150.0, 240.0) / 255.0},

    // 9. Late Afternoon: The blue begins to soften and warm up slightly as the sun descends.
    {1.0, vec3(40.0, 100.0, 220.0) / 255.0, vec3(100.0, 160.0, 255.0) / 255.0}};

// View altitude keyframes - Refined gradient for a more natural horizon
// More control points are added for a smoother blend from horizon to zenith.
const int VIEW_KEYFRAME_COUNT                                  = 6;
const ViewAltitudeKeyframe VIEW_KEYFRAMES[VIEW_KEYFRAME_COUNT] = {
    // Looking straight down - full bottom color
    {-1.0, 0.0},
    // Below horizon - blend starts subtly
    {-0.2, 0.05},
    // At the horizon line - a balanced but sharp transition
    {0.0, 0.4},
    // Just above the horizon - top color quickly becomes dominant
    {0.05, 0.6},
    // Low in the sky - mostly top color with a hint of horizon influence
    {0.3, 0.9},
    // Looking straight up (zenith) - full top color
    {1.0, 1.0}};

// Interpolate between time-of-day keyframes
SkyColors interpolate_time_keyframes(float sun_altitude) {
    SkyColors result;

    // Handle edge cases
    if (sun_altitude <= TIME_KEYFRAMES[0].sun_altitude) {
        result.top_color    = srgb_to_linear(TIME_KEYFRAMES[0].top_color);
        result.bottom_color = srgb_to_linear(TIME_KEYFRAMES[0].bottom_color);
        return result;
    }

    if (sun_altitude >= TIME_KEYFRAMES[TIME_KEYFRAME_COUNT - 1].sun_altitude) {
        result.top_color    = srgb_to_linear(TIME_KEYFRAMES[TIME_KEYFRAME_COUNT - 1].top_color);
        result.bottom_color = srgb_to_linear(TIME_KEYFRAMES[TIME_KEYFRAME_COUNT - 1].bottom_color);
        return result;
    }

    // Find the two keyframes to interpolate between
    for (int i = 0; i < TIME_KEYFRAME_COUNT - 1; i++) {
        if (sun_altitude >= TIME_KEYFRAMES[i].sun_altitude &&
            sun_altitude < TIME_KEYFRAMES[i + 1].sun_altitude) {
            float t = (sun_altitude - TIME_KEYFRAMES[i].sun_altitude) /
                      (TIME_KEYFRAMES[i + 1].sun_altitude - TIME_KEYFRAMES[i].sun_altitude);

            result.top_color = srgb_to_linear(
                mix(TIME_KEYFRAMES[i].top_color, TIME_KEYFRAMES[i + 1].top_color, t));
            result.bottom_color = srgb_to_linear(
                mix(TIME_KEYFRAMES[i].bottom_color, TIME_KEYFRAMES[i + 1].bottom_color, t));
            return result;
        }
    }

    // Fallback (shouldn't reach here)
    result.top_color    = srgb_to_linear(TIME_KEYFRAMES[0].top_color);
    result.bottom_color = srgb_to_linear(TIME_KEYFRAMES[0].bottom_color);
    return result;
}

// Interpolate view altitude blend factor
float interpolate_view_altitude(float view_altitude) {
    // Handle edge cases
    if (view_altitude <= VIEW_KEYFRAMES[0].view_altitude) {
        return VIEW_KEYFRAMES[0].blend_factor;
    }

    if (view_altitude >= VIEW_KEYFRAMES[VIEW_KEYFRAME_COUNT - 1].view_altitude) {
        return VIEW_KEYFRAMES[VIEW_KEYFRAME_COUNT - 1].blend_factor;
    }

    // Find the two keyframes to interpolate between
    for (int i = 0; i < VIEW_KEYFRAME_COUNT - 1; i++) {
        if (view_altitude >= VIEW_KEYFRAMES[i].view_altitude &&
            view_altitude < VIEW_KEYFRAMES[i + 1].view_altitude) {
            float t = (view_altitude - VIEW_KEYFRAMES[i].view_altitude) /
                      (VIEW_KEYFRAMES[i + 1].view_altitude - VIEW_KEYFRAMES[i].view_altitude);

            return mix(VIEW_KEYFRAMES[i].blend_factor, VIEW_KEYFRAMES[i + 1].blend_factor, t);
        }
    }

    // Fallback (shouldn't reach here)
    return VIEW_KEYFRAMES[0].blend_factor;
}

// Sun altitude ranges from -1 to 1
SkyColors get_sky_color_by_sun_altitude(float sun_altitude) {
    return interpolate_time_keyframes(sun_altitude);
}

vec3 get_sky_color(vec3 view_dir, vec3 sun_dir, uint use_debug_sky_colors, vec3 debug_color_1,
                   vec3 debug_color_2) {
    // Altitude range now matches sun altitude range (-1 to 1)
    float altitude     = view_dir.y;
    float sun_altitude = sun_dir.y;

    SkyColors sky_colors;
    if (use_debug_sky_colors != 0) {
        sky_colors.top_color    = srgb_to_linear(debug_color_1);
        sky_colors.bottom_color = srgb_to_linear(debug_color_2);
    } else {
        sky_colors = get_sky_color_by_sun_altitude(sun_altitude);
        // Note: srgb_to_linear is already applied in interpolate_time_keyframes
    }

    // Use keyframe-based view altitude interpolation
    float blend_factor = interpolate_view_altitude(altitude);
    blend_factor       = smoothstep(0.0, 1.0, blend_factor);
    vec3 sky_color     = mix(sky_colors.bottom_color, sky_colors.top_color, blend_factor);

    return sky_color;
}

vec3 get_sky_color_with_sun(vec3 view_dir, vec3 sun_dir, vec3 sun_color, float sun_luminance,
                            float sun_size, uint use_debug_sky_colors, vec3 debug_color_1,
                            vec3 debug_color_2) {
    vec3 sky_color_linear =
        get_sky_color(view_dir, sun_dir, use_debug_sky_colors, debug_color_1, debug_color_2);

    vec3 luminance_sun_color = sun_color * sun_luminance;
    float sun_intensity      = max(0.0, dot(view_dir, sun_dir));
    float sun_power          = max(1.0, 100.0 / max(0.01, sun_size * 10.0));
    sun_intensity            = pow(sun_intensity, sun_power);

    return mix(sky_color_linear, luminance_sun_color, sun_intensity);
}

#endif // SKYLIGHT_GLSL
