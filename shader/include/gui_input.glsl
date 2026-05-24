#ifndef GUI_INPUT_GLSL
#define GUI_INPUT_GLSL

#ifndef GUI_INPUT_SET
#define GUI_INPUT_SET 0
#endif

#ifndef GUI_INPUT_BINDING
#define GUI_INPUT_BINDING 0
#endif

layout(set = GUI_INPUT_SET, binding = GUI_INPUT_BINDING) uniform U_GuiInput {
    float debug_float;
    uint debug_bool;
    uint debug_uint;
    vec3 flora_instance_hsv_offset_max;
    vec3 flora_voxel_hsv_offset_max;
    vec3 grass_bottom_dark;
    vec3 grass_bottom_light;
    vec3 grass_tip_dark;
    vec3 grass_tip_light;
    vec3 ocean_deep_color;
    vec3 ocean_shallow_color;
    float ocean_normal_amplitude;
    float ocean_noise_frequency;
    float ocean_time_multiplier;
    float ocean_sea_level_shift;
    float lens_flare_intensity;
    float lens_flare_sun_pixel_scale;
    uint wind_mode;
    uint wind_direction_seed;
    uint wind_strength_seed;
    uint wind_gust_seed;
    uint wind_gust_direction_seed;
    float wind_direction_frequency;
    float wind_strength_frequency;
    float wind_gust_frequency;
    float wind_gust_direction_frequency;
    uint wind_direction_octaves;
    uint wind_strength_octaves;
    uint wind_gust_octaves;
    uint wind_gust_direction_octaves;
    float wind_direction_lacunarity;
    float wind_strength_lacunarity;
    float wind_gust_lacunarity;
    float wind_gust_direction_lacunarity;
    float wind_direction_gain;
    float wind_strength_gain;
    float wind_gust_gain;
    float wind_gust_direction_gain;
    float wind_sample_scale;
    float wind_second_sample_offset_x;
    float wind_second_sample_offset_y;
    float wind_strength_offset_x;
    float wind_strength_offset_y;
    float wind_gust_offset_x;
    float wind_gust_offset_y;
    float wind_gust_direction_offset_x;
    float wind_gust_direction_offset_y;
    float wind_gust_direction_second_offset_x;
    float wind_gust_direction_second_offset_y;
    float wind_direction_time_scroll_x;
    float wind_direction_time_scroll_y;
    float wind_strength_time_scroll_x;
    float wind_strength_time_scroll_y;
    float wind_gust_time_scroll_x;
    float wind_gust_time_scroll_y;
    float wind_gust_direction_time_scroll_x;
    float wind_gust_direction_time_scroll_y;
    float wind_time_scale;
    float wind_direction_detail_strength;
    float wind_gust_direction_detail_strength;
    float wind_strength_smooth_min;
    float wind_strength_smooth_max;
    float wind_gust_smooth_min;
    float wind_gust_smooth_max;
    float wind_gust_boost;
    float wind_min_strength;
    float wind_max_strength;
    float wind_output_max_strength;
}
gui_input;

#endif // GUI_INPUT_GLSL
