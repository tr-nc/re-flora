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
    uint wind_source_count;
    vec4 wind_source_0;
    vec4 wind_source_1;
    vec4 wind_source_2;
    vec4 wind_source_3;
    vec4 wind_source_0_noise;
    vec4 wind_source_1_noise;
    vec4 wind_source_2_noise;
    vec4 wind_source_3_noise;
    vec4 wind_source_0_detail;
    vec4 wind_source_1_detail;
    vec4 wind_source_2_detail;
    vec4 wind_source_3_detail;
}
gui_input;

#endif // GUI_INPUT_GLSL
