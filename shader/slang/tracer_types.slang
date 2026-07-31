module tracer_types;

public struct U_GuiInput
{
    public uint flora_growth_override_enabled;
    public float flora_growth_override;
    public float terrain_self_shadow_tolerance_voxels;
    public float3 flora_instance_hsv_offset_max;
    public float3 flora_voxel_hsv_offset_max;
    public float3 grass_bottom_dark;
    public float3 grass_bottom_light;
    public float3 grass_tip_dark;
    public float3 grass_tip_light;
    public float3 glass_tint;
    public float glass_reflection_strength;
    public float glass_ssr_strength;
    public uint glass_ssr_steps;
    public uint glass_per_voxel_reflection;
    public float glass_ssr_min_hit_thickness_voxels;
    public float glass_ssr_footprint_pixels;
    public float glass_refraction_strength;
    public float glass_alpha;
    public float glass_glint_strength;
    public float lens_flare_intensity;
    public float lens_flare_sun_pixel_scale;
    public uint wind_source_count;
    public float wind_directional_bias_fraction;
    public float wind_turbulence_fraction;
    public float world_tick_seconds;
    public float grass_vibration_amplitude_voxels;
    public float grass_vibration_primary_speed;
    public float grass_vibration_secondary_speed;
    public float grass_natural_bend_min_voxels;
    public float grass_natural_bend_max_voxels;
    public float flora_bend_height_power;
    public float kochia_body_wind_response;
    public float kochia_branch_jelly_amplitude_voxels;
    public float kochia_branch_jelly_speed;
    public float kochia_branch_phase_spread;
    public float kochia_tip_flutter_amplitude_voxels;
    public float kochia_tip_flutter_speed;
    public float kochia_bottom_darkening;
    public float kochia_branch_value_variation;
    public float kochia_voxel_value_variation;
    public uint kochia_branch_count;
    public float kochia_bottom_diameter_voxels;
    public float kochia_waist_diameter_voxels;
    public float kochia_top_diameter_voxels;
    public float kochia_waist_height;
    public float leaf_paddle_amplitude_voxels;
    public float leaf_paddle_primary_speed;
    public float leaf_paddle_secondary_speed;
    public float leaf_paddle_amplitude_wind_start_strength;
    public float leaf_paddle_amplitude_wind_full_strength;
    public float leaf_paddle_amplitude_wind_knee_bias;
    public float leaf_paddle_frequency_wind_start_strength;
    public float leaf_paddle_frequency_wind_full_strength;
    public float leaf_paddle_frequency_wind_knee_bias;
    public float leaf_paddle_frequency_min_multiplier;
    public float leaf_paddle_frequency_max_multiplier;
    public float fruit_swing_length_voxels;
    public float fruit_swing_max_angle_radians;
    public float fruit_swing_speed;
    public float fruit_swing_speed_variation;
    public float fruit_swing_min_response;
    public float leaf_shadow_fragment_opacity;
    public float leaf_shadow_strength;
    public float leaf_shadow_min_transmittance;
    public float leaf_shadow_filter_radius_texels;
    public float leaf_transmission_strength;
    public uint clouds_enabled;
    public float cloud_coverage;
    public float cloud_density;
    public float cloud_bottom_height;
    public float cloud_top_height;
    public float cloud_shape_scale;
    public float cloud_detail_scale;
    public float cloud_detail_strength;
    public float cloud_wind_speed;
    public uint cloud_primary_steps;
    public uint cloud_light_steps;
    public float cloud_temporal_alpha;
    public float cloud_absorption;
    public float cloud_phase_eccentricity;
    public float cloud_silver_intensity;
    public float cloud_max_distance;
    public uint cloud_shadows_enabled;
    public float cloud_shadow_strength;
    public float cloud_shadow_min_transmittance;
    public uint cloud_shadow_steps;
};

public struct U_SunInfo
{
    public float3 sun_dir;
    public float sun_size;
    public float3 sun_color;
    public float sun_luminance;
    public float sun_display_luminance;
    public float sun_altitude;
    public float sun_azimuth;
};

public struct U_ShadingInfo
{
    public float4 environment_irradiance_sh_0;
    public float4 environment_irradiance_sh_1;
    public float4 environment_irradiance_sh_2;
    public float4 environment_irradiance_sh_3;
    public float4 environment_irradiance_sh_4;
    public float4 environment_irradiance_sh_5;
    public float4 environment_irradiance_sh_6;
    public float4 environment_irradiance_sh_7;
    public float4 environment_irradiance_sh_8;
    public uint environment_revision;
    public uint3 environment_probe_grid_dimensions;
    public float3 environment_probe_world_to_grid_scale;
    public uint environment_probe_uniform_field;
    public float environment_probe_visibility_bias_world;
    public uint environment_lighting_backend;
    public uint environment_lighting_backend_ready;
};

public struct U_CameraInfo
{
    public float4 pos;
    public float4x4 view_mat;
    public float4x4 view_mat_inv;
    public float4x4 proj_mat;
    public float4x4 proj_mat_inv;
    public float4x4 view_proj_mat;
    public float4x4 view_proj_mat_inv;
};

public struct U_CameraInfoPrevFrame
{
    public float4 pos;
    public float4x4 view_mat;
    public float4x4 view_mat_inv;
    public float4x4 proj_mat;
    public float4x4 proj_mat_inv;
    public float4x4 view_proj_mat;
    public float4x4 view_proj_mat_inv;
};

public struct U_ShadowCameraInfo
{
    public float4 pos;
    public float4x4 view_mat;
    public float4x4 view_mat_inv;
    public float4x4 proj_mat;
    public float4x4 proj_mat_inv;
    public float4x4 view_proj_mat;
    public float4x4 view_proj_mat_inv;
};

public struct U_EnvInfo
{
    public uint frame_serial_idx;
};

public struct U_VoxelColors
{
    public float3 dirt_color;
    public float3 sand_color;
    public float3 cherry_wood_color;
    public float3 oak_wood_color;
    public float3 rock_color;
    public float hash_color_variance;
};

public struct U_TerrainEditPreview
{
    public float3 center;
    public float radius;
    public float3 color;
    public float strength;
    public uint enabled;
    public uint shape;
};
