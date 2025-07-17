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

// Time-of-day keyframes (equivalent to current behavior)
const int TIME_KEYFRAME_COUNT = 3;
const TimeOfDayKeyframe TIME_KEYFRAMES[TIME_KEYFRAME_COUNT] = {
    { -0.2, vec3(0.0, 28.0, 56.0) / 255.0, vec3(45.0, 43.0, 60.0) / 255.0 },    // night
    { -0.1, vec3(59.0, 114.0, 180.0) / 255.0, vec3(191.0, 126.0, 145.0) / 255.0 }, // sunset
    { 0.2, vec3(88.0, 101.0, 255.0) / 255.0, vec3(186.0, 186.0, 186.0) / 255.0 }   // day
};

// View altitude keyframes (equivalent to current behavior)
const int VIEW_KEYFRAME_COUNT = 2;
const ViewAltitudeKeyframe VIEW_KEYFRAMES[VIEW_KEYFRAME_COUNT] = {
    { -0.1, 0.0 }, // bottom color
    { 0.2, 1.0 }   // top color
};

// Interpolate between time-of-day keyframes
SkyColors interpolate_time_keyframes(float sun_altitude) {
    SkyColors result;
    
    // Handle edge cases
    if (sun_altitude <= TIME_KEYFRAMES[0].sun_altitude) {
        result.top_color = srgb_to_linear(TIME_KEYFRAMES[0].top_color);
        result.bottom_color = srgb_to_linear(TIME_KEYFRAMES[0].bottom_color);
        return result;
    }
    
    if (sun_altitude >= TIME_KEYFRAMES[TIME_KEYFRAME_COUNT - 1].sun_altitude) {
        result.top_color = srgb_to_linear(TIME_KEYFRAMES[TIME_KEYFRAME_COUNT - 1].top_color);
        result.bottom_color = srgb_to_linear(TIME_KEYFRAMES[TIME_KEYFRAME_COUNT - 1].bottom_color);
        return result;
    }
    
    // Find the two keyframes to interpolate between
    for (int i = 0; i < TIME_KEYFRAME_COUNT - 1; i++) {
        if (sun_altitude >= TIME_KEYFRAMES[i].sun_altitude && sun_altitude < TIME_KEYFRAMES[i + 1].sun_altitude) {
            float t = (sun_altitude - TIME_KEYFRAMES[i].sun_altitude) / 
                      (TIME_KEYFRAMES[i + 1].sun_altitude - TIME_KEYFRAMES[i].sun_altitude);
            
            result.top_color = srgb_to_linear(mix(TIME_KEYFRAMES[i].top_color, TIME_KEYFRAMES[i + 1].top_color, t));
            result.bottom_color = srgb_to_linear(mix(TIME_KEYFRAMES[i].bottom_color, TIME_KEYFRAMES[i + 1].bottom_color, t));
            return result;
        }
    }
    
    // Fallback (shouldn't reach here)
    result.top_color = srgb_to_linear(TIME_KEYFRAMES[0].top_color);
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
        if (view_altitude >= VIEW_KEYFRAMES[i].view_altitude && view_altitude < VIEW_KEYFRAMES[i + 1].view_altitude) {
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
    blend_factor = smoothstep(0.0, 1.0, blend_factor);
    vec3 sky_color = mix(sky_colors.bottom_color, sky_colors.top_color, blend_factor);

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
