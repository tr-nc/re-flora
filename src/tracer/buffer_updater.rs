use crate::ddgi::DdgiVolumeGrid;
use crate::environment_lighting::EnvironmentLightingState;
use crate::flora::species::{species, MAX_FLORA_SPECIES};
use crate::generated::gpu_structs::{
    EnvInfo, FloraGrowthInfo, GodRayInfo, GuiInput, PlayerColliderInfo, PostProcessingInfo,
    ShadingInfo, StarlightInfo, SunInfo, TerrainEditPreview, VoxelColors,
};
use crate::tracer::{
    CloudGuiParams, FruitMotionParams, GlassGuiParams, KochiaMotionParams, KochiaVisualParams,
    TerrainEditPreviewShape, TracerResources, WindGuiParams, WindSourceGpu,
};
use anyhow::Result;
use bytemuck::Zeroable;
use glam::{Mat4, Vec3};

pub struct BufferUpdater;

impl BufferUpdater {
    pub fn update_camera_info(
        camera_info: &mut re_flora_vkn::Buffer,
        view_mat: Mat4,
        proj_mat: Mat4,
    ) -> Result<()> {
        use crate::generated::gpu_structs::CameraInfo;
        let view_proj_mat = proj_mat * view_mat;
        let camera_pos = view_mat.inverse().w_axis;
        let data = CameraInfo {
            pos: camera_pos.to_array(),
            view_mat: view_mat.to_cols_array_2d(),
            view_mat_inv: view_mat.inverse().to_cols_array_2d(),
            proj_mat: proj_mat.to_cols_array_2d(),
            proj_mat_inv: proj_mat.inverse().to_cols_array_2d(),
            view_proj_mat: view_proj_mat.to_cols_array_2d(),
            view_proj_mat_inv: view_proj_mat.inverse().to_cols_array_2d(),
        };
        camera_info.fill_uniform(&data)
    }

    pub fn update_env_info(resources: &TracerResources, frame_serial_idx: u32) -> Result<()> {
        resources.uniforms.env_info.fill_uniform(&EnvInfo {
            frame_serial_idx,
            ..EnvInfo::zeroed()
        })
    }

    pub fn update_shading_info(
        resources: &TracerResources,
        environment: EnvironmentLightingState,
        environment_probe_grid: DdgiVolumeGrid,
        voxels_per_world_unit: glam::UVec3,
        ddgi_ready: bool,
        environment_irradiance_capture_enabled: bool,
        environment_irradiance_capture_unpublished: bool,
        ddgi_irradiance_tile_columns: u32,
        ddgi_visibility_tile_columns: u32,
        ddgi_debug_view: u32,
        ddgi_invalidation_voxel_bound: Option<crate::geom::UAabb3>,
    ) -> Result<()> {
        let probe_dimensions = environment_probe_grid.dimensions();
        let probe_world_to_grid_scale =
            voxels_per_world_unit.as_vec3() / environment_probe_grid.spacing_voxels() as f32;
        let (ddgi_invalidation_enabled, ddgi_invalidation_world_min, ddgi_invalidation_world_max) =
            ddgi_invalidation_voxel_bound.map_or((0, [0.0; 3], [0.0; 3]), |bound| {
                let voxels_per_world_unit = voxels_per_world_unit.as_vec3();
                (
                    1,
                    (bound.min().as_vec3() / voxels_per_world_unit).to_array(),
                    (bound.max().as_vec3() / voxels_per_world_unit).to_array(),
                )
            });
        resources.uniforms.shading_info.fill_uniform(&ShadingInfo {
            environment_revision: environment.revision,
            environment_probe_grid_dimensions: probe_dimensions.to_array(),
            environment_probe_world_to_grid_scale: probe_world_to_grid_scale.to_array(),
            environment_probe_visibility_bias_world: 2.0
                / voxels_per_world_unit.min_element().max(1) as f32,
            ddgi_ready: u32::from(ddgi_ready),
            environment_irradiance_capture_enabled: u32::from(
                environment_irradiance_capture_enabled,
            ),
            environment_irradiance_capture_unpublished: u32::from(
                environment_irradiance_capture_unpublished,
            ),
            ddgi_irradiance_tile_columns,
            ddgi_visibility_tile_columns,
            ddgi_debug_view,
            ddgi_invalidation_enabled,
            ddgi_invalidation_world_min,
            ddgi_invalidation_world_max,
            ..ShadingInfo::zeroed()
        })
    }

    pub fn update_post_processing_info(
        resources: &TracerResources,
        scaling_factor: f32,
    ) -> Result<()> {
        resources
            .uniforms
            .post_processing_info
            .fill_uniform(&PostProcessingInfo {
                scaling_factor,
                ..PostProcessingInfo::zeroed()
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_flora_growth_info(
        resources: &TracerResources,
        flora_tick: u32,
        sprout_delay_ticks: u32,
        full_growth_ticks: u32,
        spawn_time_ms: u32,
        spawn_duration_seconds: f32,
        spawn_rise_fraction: f32,
        spawn_overshoot_min_voxels: f32,
        spawn_overshoot_max_voxels: f32,
        spawn_stagger_seconds: f32,
    ) -> Result<()> {
        let moisture_growth_factors: [u32; MAX_FLORA_SPECIES * 4] = std::array::from_fn(|idx| {
            species()[idx / 4].moisture_growth_factors[idx % 4].to_bits()
        });
        resources
            .uniforms
            .flora_growth_info
            .fill_uniform(&FloraGrowthInfo {
                flora_tick,
                sprout_delay_ticks,
                full_growth_ticks,
                spawn_time_ms,
                spawn_duration_seconds,
                spawn_rise_fraction,
                spawn_overshoot_min_voxels,
                spawn_overshoot_max_voxels,
                spawn_stagger_seconds,
                moisture_growth_factors,
                ..FloraGrowthInfo::zeroed()
            })
    }

    pub fn update_god_ray_info(
        resources: &TracerResources,
        max_depth: f32,
        max_checks: u32,
        weight: f32,
        color: Vec3,
    ) -> Result<()> {
        resources.uniforms.god_ray_info.fill_uniform(&GodRayInfo {
            max_depth,
            max_checks,
            weight,
            color: color.to_array(),
            ..GodRayInfo::zeroed()
        })
    }

    #[allow(dead_code)]
    pub fn update_player_collider_info(
        resources: &TracerResources,
        player_pos: Vec3,
        camera_front: Vec3,
    ) -> Result<()> {
        resources
            .terrain_query
            .player_collider_info
            .fill_uniform(&PlayerColliderInfo {
                player_pos: player_pos.to_array(),
                camera_front: camera_front.to_array(),
                ..PlayerColliderInfo::zeroed()
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_sun_info(
        resources: &TracerResources,
        sun_dir: Vec3,
        sun_size: f32,
        sun_color: Vec3,
        sun_luminance: f32,
        sun_display_luminance: f32,
        sun_altitude: f32,
        sun_azimuth: f32,
    ) -> Result<()> {
        resources.uniforms.sun_info.fill_uniform(&SunInfo {
            sun_dir: sun_dir.to_array(),
            sun_size,
            sun_color: sun_color.to_array(),
            sun_luminance,
            sun_display_luminance,
            sun_altitude,
            sun_azimuth,
            ..SunInfo::zeroed()
        })
    }

    pub fn update_voxel_colors(
        resources: &TracerResources,
        dirt_color: Vec3,
        sand_color: Vec3,
        cherry_wood_color: Vec3,
        oak_wood_color: Vec3,
        rock_color: Vec3,
        hash_color_variance: f32,
    ) -> Result<()> {
        resources.uniforms.voxel_colors.fill_uniform(&VoxelColors {
            dirt_color: dirt_color.to_array(),
            sand_color: sand_color.to_array(),
            cherry_wood_color: cherry_wood_color.to_array(),
            oak_wood_color: oak_wood_color.to_array(),
            rock_color: rock_color.to_array(),
            hash_color_variance,
            ..VoxelColors::zeroed()
        })
    }

    pub fn update_terrain_edit_preview(
        resources: &TracerResources,
        center: Option<Vec3>,
        radius: f32,
        shape: TerrainEditPreviewShape,
        color: Vec3,
        alpha: f32,
    ) -> Result<()> {
        let enabled = center.is_some() && radius > 0.0;
        resources
            .uniforms
            .terrain_edit_preview
            .fill_uniform(&TerrainEditPreview {
                center: center.unwrap_or(Vec3::ZERO).to_array(),
                radius: radius.max(0.0),
                color: color.clamp(Vec3::ZERO, Vec3::ONE).to_array(),
                strength: alpha.clamp(0.0, 1.0),
                enabled: enabled as u32,
                shape: shape.as_u32(),
                ..TerrainEditPreview::zeroed()
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_gui_input(
        resources: &TracerResources,
        flora_growth_override_enabled: bool,
        flora_growth_override: f32,
        terrain_self_shadow_tolerance_voxels: f32,
        flora_instance_hsv_offset_max: Vec3,
        flora_voxel_hsv_offset_max: Vec3,
        grass_bottom_dark: Vec3,
        grass_bottom_light: Vec3,
        grass_tip_dark: Vec3,
        grass_tip_light: Vec3,
        lens_flare_intensity: f32,
        lens_flare_sun_pixel_scale: f32,
        glass_gui_params: GlassGuiParams,
        wind_directional_bias_fraction: f32,
        wind_turbulence_fraction: f32,
        world_tick_seconds: f32,
        grass_vibration_amplitude_voxels: f32,
        grass_vibration_primary_speed: f32,
        grass_vibration_secondary_speed: f32,
        grass_natural_bend_min_voxels: f32,
        grass_natural_bend_max_voxels: f32,
        flora_bend_height_power: f32,
        kochia_motion: KochiaMotionParams,
        kochia_visual: KochiaVisualParams,
        leaf_paddle_amplitude_voxels: f32,
        leaf_paddle_primary_speed: f32,
        leaf_paddle_secondary_speed: f32,
        leaf_paddle_amplitude_wind_start_strength: f32,
        leaf_paddle_amplitude_wind_full_strength: f32,
        leaf_paddle_amplitude_wind_knee_bias: f32,
        leaf_paddle_frequency_wind_start_strength: f32,
        leaf_paddle_frequency_wind_full_strength: f32,
        leaf_paddle_frequency_wind_knee_bias: f32,
        leaf_paddle_frequency_min_multiplier: f32,
        leaf_paddle_frequency_max_multiplier: f32,
        fruit_motion: FruitMotionParams,
        leaf_shadow_fragment_opacity: f32,
        leaf_shadow_strength: f32,
        leaf_shadow_min_transmittance: f32,
        leaf_shadow_filter_radius_texels: f32,
        leaf_transmission_strength: f32,
        wind_gui_params: WindGuiParams,
        cloud_gui_params: CloudGuiParams,
    ) -> Result<()> {
        let wind_source_count = wind_gui_params.sources.len() as u32;
        let mut wind_sources = wind_gui_params
            .sources
            .iter()
            .copied()
            .map(WindSourceGpu::from)
            .collect::<Vec<_>>();
        if wind_sources.is_empty() {
            wind_sources.push(WindSourceGpu::zeroed());
        }
        resources.wind.wind_sources.fill(&wind_sources)?;

        resources.uniforms.gui_input.fill_uniform(&GuiInput {
            flora_growth_override_enabled: flora_growth_override_enabled as u32,
            flora_growth_override: flora_growth_override.clamp(0.0, 1.0),
            terrain_self_shadow_tolerance_voxels,
            flora_instance_hsv_offset_max: flora_instance_hsv_offset_max.to_array(),
            flora_voxel_hsv_offset_max: flora_voxel_hsv_offset_max.to_array(),
            grass_bottom_dark: grass_bottom_dark.to_array(),
            grass_bottom_light: grass_bottom_light.to_array(),
            grass_tip_dark: grass_tip_dark.to_array(),
            grass_tip_light: grass_tip_light.to_array(),
            glass_tint: glass_gui_params.tint.to_array(),
            glass_reflection_strength: glass_gui_params.reflection_strength,
            glass_ssr_strength: glass_gui_params.ssr_strength,
            glass_ssr_steps: glass_gui_params.ssr_steps,
            glass_per_voxel_reflection: glass_gui_params.per_voxel_reflection as u32,
            glass_ssr_min_hit_thickness_voxels: glass_gui_params.ssr_min_hit_thickness_voxels,
            glass_ssr_footprint_pixels: glass_gui_params.ssr_footprint_pixels,
            glass_refraction_strength: glass_gui_params.refraction_strength,
            glass_alpha: glass_gui_params.alpha,
            glass_glint_strength: glass_gui_params.glint_strength,
            lens_flare_intensity,
            lens_flare_sun_pixel_scale,
            wind_source_count,
            wind_directional_bias_fraction,
            wind_turbulence_fraction,
            world_tick_seconds,
            grass_vibration_amplitude_voxels,
            grass_vibration_primary_speed,
            grass_vibration_secondary_speed,
            grass_natural_bend_min_voxels,
            grass_natural_bend_max_voxels,
            flora_bend_height_power,
            kochia_body_wind_response: kochia_motion.body_wind_response,
            kochia_branch_jelly_amplitude_voxels: kochia_motion.branch_jelly_amplitude_voxels,
            kochia_branch_jelly_speed: kochia_motion.branch_jelly_speed,
            kochia_branch_phase_spread: kochia_motion.branch_phase_spread,
            kochia_tip_flutter_amplitude_voxels: kochia_motion.tip_flutter_amplitude_voxels,
            kochia_tip_flutter_speed: kochia_motion.tip_flutter_speed,
            kochia_bottom_darkening: kochia_visual.bottom_darkening,
            kochia_branch_value_variation: kochia_visual.branch_value_variation,
            kochia_voxel_value_variation: kochia_visual.voxel_value_variation,
            kochia_branch_count: kochia_visual.branch_count,
            kochia_bottom_diameter_voxels: kochia_visual.bottom_diameter_voxels,
            kochia_waist_diameter_voxels: kochia_visual.waist_diameter_voxels,
            kochia_top_diameter_voxels: kochia_visual.top_diameter_voxels,
            kochia_waist_height: kochia_visual.waist_height,
            leaf_paddle_amplitude_voxels,
            leaf_paddle_primary_speed,
            leaf_paddle_secondary_speed,
            leaf_paddle_amplitude_wind_start_strength,
            leaf_paddle_amplitude_wind_full_strength,
            leaf_paddle_amplitude_wind_knee_bias,
            leaf_paddle_frequency_wind_start_strength,
            leaf_paddle_frequency_wind_full_strength,
            leaf_paddle_frequency_wind_knee_bias,
            leaf_paddle_frequency_min_multiplier,
            leaf_paddle_frequency_max_multiplier,
            fruit_swing_length_voxels: fruit_motion.swing_length_voxels,
            fruit_swing_max_angle_radians: fruit_motion.max_angle_radians,
            fruit_swing_speed: fruit_motion.swing_speed,
            fruit_swing_speed_variation: fruit_motion.speed_variation,
            fruit_swing_min_response: fruit_motion.min_response,
            leaf_shadow_fragment_opacity,
            leaf_shadow_strength,
            leaf_shadow_min_transmittance,
            leaf_shadow_filter_radius_texels,
            leaf_transmission_strength,
            clouds_enabled: cloud_gui_params.enabled as u32,
            cloud_coverage: cloud_gui_params.coverage,
            cloud_density: cloud_gui_params.density,
            cloud_bottom_height: cloud_gui_params.bottom_height,
            cloud_top_height: cloud_gui_params.top_height,
            cloud_shape_scale: cloud_gui_params.shape_scale,
            cloud_detail_scale: cloud_gui_params.detail_scale,
            cloud_detail_strength: cloud_gui_params.detail_strength,
            cloud_wind_speed: cloud_gui_params.wind_speed,
            cloud_primary_steps: cloud_gui_params.primary_steps,
            cloud_light_steps: cloud_gui_params.light_steps,
            cloud_temporal_alpha: cloud_gui_params.temporal_alpha,
            cloud_absorption: cloud_gui_params.absorption,
            cloud_phase_eccentricity: cloud_gui_params.phase_eccentricity,
            cloud_silver_intensity: cloud_gui_params.silver_intensity,
            cloud_max_distance: cloud_gui_params.max_distance,
            cloud_shadows_enabled: cloud_gui_params.shadows_enabled as u32,
            cloud_shadow_strength: cloud_gui_params.shadow_strength,
            cloud_shadow_min_transmittance: cloud_gui_params.shadow_min_transmittance,
            cloud_shadow_steps: cloud_gui_params.shadow_steps,
            ..GuiInput::zeroed()
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_starlight_info(
        resources: &TracerResources,
        iterations: i32,
        formuparam: f32,
        volsteps: i32,
        stepsize: f32,
        zoom: f32,
        tile: f32,
        speed: f32,
        brightness: f32,
        darkmatter: f32,
        distfading: f32,
        saturation: f32,
    ) -> Result<()> {
        resources
            .uniforms
            .starlight_info
            .fill_uniform(&StarlightInfo {
                iterations,
                formuparam,
                volsteps,
                stepsize,
                zoom,
                tile,
                speed,
                brightness,
                darkmatter,
                distfading,
                saturation,
                ..StarlightInfo::zeroed()
            })
    }
}
