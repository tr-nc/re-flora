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
const int TIME_KEYFRAME_COUNT                               = 10;
const TimeOfDayKeyframe TIME_KEYFRAMES[TIME_KEYFRAME_COUNT] = {
    // 1. Deep Night: Almost pitch black with a hint of deep, cold blue.
    {-0.4, vec3(4.0, 5.0, 10.0) / 255.0, vec3(8.0, 10.0, 20.0) / 255.0},

    // 2. Pre-Dawn: The first light appears as a subtle purple/magenta glow on the horizon.
    {-0.18, vec3(20.0, 25.0, 45.0) / 255.0, vec3(40.0, 30.0, 50.0) / 255.0},

    // 3. Blue Hour: The sky is filled with a deep, saturated blue.
    // NEW: This keyframe creates a distinct blue-dominated period during twilight.
    {-0.1, vec3(15.0, 25.0, 60.0) / 255.0, vec3(30.0, 25.0, 60.0) / 255.0},

    // 4. Dawn/Dusk Glow: The sky above is a deep blue, while the horizon burns with a vibrant red
    // glow.
    {-0.05, vec3(30.0, 50.0, 100.0) / 255.0, vec3(180.0, 50.0, 90.0) / 255.0},

    // 5. Sunrise/Sunset: The sun is at the horizon. The top of the sky is a lighter blue, meeting a
    // fiery orange.
    {0.0, vec3(80.0, 100.0, 180.0) / 255.0, vec3(255.0, 100.0, 40.0) / 255.0},

    // 6. Golden Hour: As the sun rises, the fiery reds give way to a warm, golden light.
    {0.1, vec3(135.0, 170.0, 255.0) / 255.0, vec3(255.0, 180.0, 100.0) / 255.0},

    // 7. Morning: The sky clears to a crisp, welcoming blue. A pale cyan near the horizon suggests
    // morning haze.
    {0.3, vec3(100.0, 160.0, 255.0) / 255.0, vec3(180.0, 210.0, 255.0) / 255.0},

    // 8. Mid-day: A classic, bright sky blue. The variation between top and bottom adds depth.
    {0.5, vec3(80.0, 150.0, 255.0) / 255.0, vec3(110.0, 170.0, 245.0) / 255.0},

    // 9. High Noon: At its peak, the sun makes the zenith a deep, pure blue, creating a subtle
    // daytime gradient.
    {0.8, vec3(60.0, 120.0, 230.0) / 255.0, vec3(90.0, 150.0, 240.0) / 255.0},

    // 10. Late Afternoon: The blue begins to soften and warm up slightly as the sun descends.
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

vec3 get_sky_color(vec3 view_dir, vec3 sun_dir) {
    // Altitude range now matches sun altitude range (-1 to 1)
    float altitude     = view_dir.y;
    float sun_altitude = sun_dir.y;

    SkyColors sky_colors = get_sky_color_by_sun_altitude(sun_altitude);

    // Use keyframe-based view altitude interpolation
    float blend_factor  = interpolate_view_altitude(altitude);
    blend_factor        = smoothstep(0.0, 1.0, blend_factor);
    vec3 base_sky_color = mix(sky_colors.bottom_color, sky_colors.top_color, blend_factor);

    // Add sun-halo effect
    float sun_proximity = max(0.0, dot(view_dir, sun_dir));

    // Derive halo color from sky color at sun's position
    float sun_blend_factor = interpolate_view_altitude(sun_altitude);
    sun_blend_factor       = smoothstep(0.0, 1.0, sun_blend_factor);
    vec3 halo_color        = mix(sky_colors.bottom_color, sky_colors.top_color, sun_blend_factor);

    // Halo falloff - stronger when sun is low, creates nice sunset halos
    float halo_size     = mix(0.1, 0.05, abs(sun_altitude)); // Larger halo when sun is low
    float halo_strength = pow(sun_proximity, 1.0 / halo_size);
    halo_strength *= (1.0 - abs(sun_altitude) * 0.5); // Reduce halo at zenith

    // Blend halo with base sky color
    vec3 sky_color = mix(base_sky_color, halo_color, halo_strength * 0.4);

    return sky_color;
}

vec3 get_sky_color_with_sun(vec3 view_dir, vec3 sun_dir, vec3 sun_color, float sun_luminance,
                            float sun_size) {
    vec3 sky_color_linear = get_sky_color(view_dir, sun_dir);
    float sun_dist        = 1.0 - dot(view_dir, sun_dir);
    sun_dist /= sun_size;

    float sun = 0.05 / max(sun_dist, 0.001) + 0.02;

    vec3 sun_contribution = vec3(sun / 0.477, sun + 0.5, sun + 0.8);

    // Scale by sun luminance and apply size factor
    sun_contribution *= sun_luminance * 0.2;

    // Blend the sun contribution with the base sky color
    vec3 luminance_sun_color = sun_color * sun_contribution;

    // Use a falloff based on distance for smooth blending
    float sun_blend_factor = clamp(sun * 0.1, 0.0, 1.0);

    return mix(sky_color_linear, luminance_sun_color, sun_blend_factor);
}

#endif // SKYLIGHT_GLSL
