#ifndef GUI_INPUT_GLSL
#define GUI_INPUT_GLSL

#ifndef GUI_INPUT_SET
#define GUI_INPUT_SET 0
#endif

#ifndef GUI_INPUT_BINDING
#define GUI_INPUT_BINDING 0
#endif

#ifndef GUI_INPUT_NAME
#define GUI_INPUT_NAME gui_input
#endif

layout(set = GUI_INPUT_SET, binding = GUI_INPUT_BINDING) uniform U_GuiInput {
    uint debug_bool;
    uint flora_growth_override_enabled;
    float flora_growth_override;
    uint terrain_shadow_use_vsm;
    vec3 flora_instance_hsv_offset_max;
    vec3 flora_voxel_hsv_offset_max;
    vec3 grass_bottom_dark;
    vec3 grass_bottom_light;
    vec3 grass_tip_dark;
    vec3 grass_tip_light;
    vec3 glass_tint;
    float glass_reflection_strength;
    float glass_ssr_strength;
    uint glass_ssr_steps;
    uint glass_per_voxel_reflection;
    float glass_ssr_min_hit_thickness_voxels;
    float glass_ssr_footprint_pixels;
    float glass_refraction_strength;
    float glass_alpha;
    float glass_glint_strength;
    float lens_flare_intensity;
    float lens_flare_sun_pixel_scale;
    uint wind_source_count;
    float wind_directional_bias_fraction;
    float wind_turbulence_fraction;
    float world_tick_seconds;
    float grass_vibration_amplitude_voxels;
    float grass_vibration_primary_speed;
    float grass_vibration_secondary_speed;
    float grass_natural_bend_min_voxels;
    float grass_natural_bend_max_voxels;
    float flora_bend_height_power;
    float kochia_body_wind_response;
    float kochia_branch_jelly_amplitude_voxels;
    float kochia_branch_jelly_speed;
    float kochia_branch_phase_spread;
    float kochia_tip_flutter_amplitude_voxels;
    float kochia_tip_flutter_speed;
    float kochia_bottom_darkening;
    float kochia_branch_value_variation;
    float kochia_voxel_value_variation;
    uint kochia_branch_count;
    float leaf_paddle_amplitude_voxels;
    float leaf_paddle_primary_speed;
    float leaf_paddle_secondary_speed;
    float leaf_paddle_amplitude_wind_start_strength;
    float leaf_paddle_amplitude_wind_full_strength;
    float leaf_paddle_amplitude_wind_knee_bias;
    float leaf_paddle_frequency_wind_start_strength;
    float leaf_paddle_frequency_wind_full_strength;
    float leaf_paddle_frequency_wind_knee_bias;
    float leaf_paddle_frequency_min_multiplier;
    float leaf_paddle_frequency_max_multiplier;
    float fruit_swing_length_voxels;
    float fruit_swing_max_angle_radians;
    float fruit_swing_speed;
    float fruit_swing_speed_variation;
    float fruit_swing_min_response;
    float leaf_shadow_fragment_opacity;
    float leaf_shadow_strength;
    float leaf_shadow_min_transmittance;
    float leaf_shadow_filter_radius_texels;
    uint clouds_enabled;
    float cloud_coverage;
    float cloud_density;
    float cloud_bottom_height;
    float cloud_top_height;
    float cloud_shape_scale;
    float cloud_detail_scale;
    float cloud_detail_strength;
    float cloud_wind_speed;
    uint cloud_primary_steps;
    uint cloud_light_steps;
    float cloud_temporal_alpha;
    float cloud_absorption;
    float cloud_phase_eccentricity;
    float cloud_silver_intensity;
    float cloud_max_distance;
    uint cloud_shadows_enabled;
    float cloud_shadow_strength;
    float cloud_shadow_min_transmittance;
    uint cloud_shadow_steps;
}
GUI_INPUT_NAME;

#endif // GUI_INPUT_GLSL
