use super::{FLORA_FULL_GROWTH_TICKS, FLORA_SPROUT_DELAY_TICKS};
use crate::app::{DebugSettings, GuiAdjustables};
use crate::tracer::{
    CloudGuiParams, EnvironmentFrameInput, FloraAppearanceFrameInput, FloraGrowthFrameInput,
    FloraMotionFrameInput, FruitMotionParams, GlassGuiParams, GodRayFrameInput, KochiaMotionParams,
    KochiaVisualParams, LeafLightingFrameInput, MaterialFrameInput, StarlightFrameInput,
    SunFrameInput, TerrainEditPreviewShape, TerrainFrameInput, VegetationFrameInput,
    WindFrameInput, WindGuiParams,
};
use egui::Color32;
use glam::Vec3;

/// Per-frame facts whose owners are outside GUI configuration.
pub(super) struct LiveRenderFrameFacts {
    pub world_tick_seconds: f32,
    pub flora_tick: u32,
    pub visual_time_since_start: f32,
    pub sun_direction: Vec3,
    pub sun_altitude: f32,
    pub sun_azimuth: f32,
    pub terrain_edit_preview_center: Option<Vec3>,
    pub terrain_edit_preview_radius: f32,
    pub terrain_edit_preview_shape: TerrainEditPreviewShape,
    pub terrain_edit_preview_color: Vec3,
    pub terrain_edit_preview_alpha: f32,
}

/// The application-side transaction of independently evolving renderer snapshots.
pub(super) struct FrozenRenderFrameInputs {
    pub terrain: TerrainFrameInput,
    pub materials: MaterialFrameInput,
    pub vegetation: VegetationFrameInput,
    pub wind: WindFrameInput,
    pub environment: EnvironmentFrameInput,
}

fn color_to_vec3(color: Color32) -> Vec3 {
    Vec3::new(
        color.r() as f32 / 255.0,
        color.g() as f32 / 255.0,
        color.b() as f32 / 255.0,
    )
}

pub(super) fn freeze_render_frame_inputs(
    settings: &DebugSettings,
    live: LiveRenderFrameFacts,
) -> FrozenRenderFrameInputs {
    let gui = &settings.adjustables;
    let terrain = TerrainFrameInput {
        ray_origin_offset_world: gui.terrain_ray_origin_offset_world.value,
        ddgi_receiver_visibility_bias_world: gui.ddgi_receiver_visibility_bias_world.value,
        ddgi_history_retention: gui.ddgi_history_retention.value,
        self_shadow_tolerance_voxels: gui.terrain_self_shadow_tolerance_voxels.value,
        edit_preview_center: live.terrain_edit_preview_center,
        edit_preview_radius: live.terrain_edit_preview_radius,
        edit_preview_shape: live.terrain_edit_preview_shape,
        edit_preview_color: live.terrain_edit_preview_color,
        edit_preview_alpha: live.terrain_edit_preview_alpha,
    };
    let materials = MaterialFrameInput {
        glass: GlassGuiParams {
            tint: color_to_vec3(gui.glass_tint.value),
            reflection_strength: gui.glass_reflection_strength.value,
            ssr_strength: gui.glass_ssr_strength.value,
            ssr_steps: gui.glass_ssr_steps.value,
            per_voxel_reflection: gui.glass_per_voxel_reflection.value,
            ssr_min_hit_thickness_voxels: gui.glass_ssr_min_hit_thickness_voxels.value,
            ssr_footprint_pixels: gui.glass_ssr_footprint_pixels.value,
            refraction_strength: gui.glass_refraction_strength.value,
            alpha: gui.glass_alpha.value,
            glint_strength: gui.glass_glint_strength.value,
        },
        voxel_dirt_color: color_to_vec3(gui.voxel_dirt_color.value),
        voxel_sand_color: color_to_vec3(gui.voxel_sand_color.value),
        voxel_cherry_wood_color: color_to_vec3(gui.voxel_cherry_wood_color.value),
        voxel_oak_wood_color: color_to_vec3(gui.voxel_oak_wood_color.value),
        voxel_rock_color: color_to_vec3(gui.voxel_rock_color.value),
        voxel_color_variance: gui.voxel_color_variance.value,
    };
    let vegetation = VegetationFrameInput {
        appearance: FloraAppearanceFrameInput {
            growth_override_enabled: gui.flora_growth_override_enabled.value,
            growth_override: gui.flora_growth_override.value,
            instance_hsv_offset_max: Vec3::new(
                gui.flora_instance_hue_offset.value,
                gui.flora_instance_saturation_offset.value,
                gui.flora_instance_value_offset.value,
            ),
            voxel_hsv_offset_max: Vec3::new(
                gui.flora_voxel_hue_offset.value,
                gui.flora_voxel_saturation_offset.value,
                gui.flora_voxel_value_offset.value,
            ),
            grass_bottom_dark: color_to_vec3(gui.grass_bottom_dark_color.value),
            grass_bottom_light: color_to_vec3(gui.grass_bottom_light_color.value),
            grass_tip_dark: color_to_vec3(gui.grass_tip_dark_color.value),
            grass_tip_light: color_to_vec3(gui.grass_tip_light_color.value),
            kochia: KochiaVisualParams {
                bottom_darkening: gui.kochia_bottom_darkening.value,
                branch_value_variation: gui.kochia_branch_value_variation.value,
                voxel_value_variation: gui.kochia_voxel_value_variation.value,
                branch_count: gui.kochia_branch_count.value,
                bottom_diameter_voxels: gui.kochia_bottom_diameter_voxels.value,
                waist_diameter_voxels: gui.kochia_waist_diameter_voxels.value,
                top_diameter_voxels: gui.kochia_top_diameter_voxels.value,
                waist_height: gui.kochia_waist_height.value,
            },
        },
        motion: FloraMotionFrameInput {
            world_tick_seconds: live.world_tick_seconds,
            grass_vibration_amplitude_voxels: gui.grass_vibration_amplitude_voxels.value,
            grass_vibration_primary_speed: gui.grass_vibration_primary_speed.value,
            grass_vibration_secondary_speed: gui.grass_vibration_secondary_speed.value,
            grass_natural_bend_min_voxels: gui.grass_natural_bend_min_voxels.value,
            grass_natural_bend_max_voxels: gui.grass_natural_bend_max_voxels.value,
            bend_height_power: gui.flora_bend_height_power.value,
            kochia: KochiaMotionParams {
                body_wind_response: gui.kochia_body_wind_response.value,
                branch_jelly_amplitude_voxels: gui.kochia_branch_jelly_amplitude_voxels.value,
                branch_jelly_speed: gui.kochia_branch_jelly_speed.value,
                branch_phase_spread: gui.kochia_branch_phase_spread.value,
                tip_flutter_amplitude_voxels: gui.kochia_tip_flutter_amplitude_voxels.value,
                tip_flutter_speed: gui.kochia_tip_flutter_speed.value,
            },
            leaf_paddle_amplitude_voxels: gui.leaf_paddle_amplitude_voxels.value,
            leaf_paddle_primary_speed: gui.leaf_paddle_primary_speed.value,
            leaf_paddle_secondary_speed: gui.leaf_paddle_secondary_speed.value,
            leaf_paddle_amplitude_wind_start_strength: gui
                .leaf_paddle_amplitude_wind_start_strength
                .value,
            leaf_paddle_amplitude_wind_full_strength: gui
                .leaf_paddle_amplitude_wind_full_strength
                .value,
            leaf_paddle_amplitude_wind_knee_bias: gui.leaf_paddle_amplitude_wind_knee_bias.value,
            leaf_paddle_frequency_wind_start_strength: gui
                .leaf_paddle_frequency_wind_start_strength
                .value,
            leaf_paddle_frequency_wind_full_strength: gui
                .leaf_paddle_frequency_wind_full_strength
                .value,
            leaf_paddle_frequency_wind_knee_bias: gui.leaf_paddle_frequency_wind_knee_bias.value,
            leaf_paddle_frequency_min_multiplier: gui.leaf_paddle_frequency_min_multiplier.value,
            leaf_paddle_frequency_max_multiplier: gui.leaf_paddle_frequency_max_multiplier.value,
            fruit: FruitMotionParams {
                swing_length_voxels: settings.tree.desc.fruit_swing_length_voxels,
                max_angle_radians: settings
                    .tree
                    .desc
                    .fruit_swing_max_angle_degrees
                    .to_radians(),
                swing_speed: settings.tree.desc.fruit_swing_speed,
                speed_variation: settings.tree.desc.fruit_swing_speed_variation,
                min_response: settings.tree.desc.fruit_swing_min_response,
            },
        },
        leaf_lighting: LeafLightingFrameInput {
            shadow_fragment_opacity: gui.leaf_shadow_fragment_opacity.value,
            shadow_strength: gui.leaf_shadow_strength.value,
            shadow_min_transmittance: gui.leaf_shadow_min_transmittance.value,
            shadow_filter_radius_texels: gui.leaf_shadow_filter_radius_texels.value,
            transmission_strength: gui.leaf_transmission_strength.value,
        },
        growth: FloraGrowthFrameInput {
            flora_tick: live.flora_tick,
            sprout_delay_ticks: FLORA_SPROUT_DELAY_TICKS,
            full_growth_ticks: FLORA_FULL_GROWTH_TICKS,
            spawn_time_ms: (live.visual_time_since_start * 1000.0) as u32,
            spawn_duration_seconds: gui.flora_spawn_duration_seconds.value,
            spawn_rise_fraction: gui.flora_spawn_rise_fraction.value,
            spawn_overshoot_min_voxels: gui.flora_spawn_overshoot_min_voxels.value,
            spawn_overshoot_max_voxels: gui.flora_spawn_overshoot_max_voxels.value,
            spawn_stagger_seconds: gui.flora_spawn_stagger_seconds.value,
        },
    };
    let wind = WindFrameInput {
        sources: WindGuiParams {
            sources: GuiAdjustables::active_wind_sources(&settings.wind_sources),
        },
        directional_bias_fraction: gui.wind_directional_bias_fraction.value,
        turbulence_fraction: gui.wind_turbulence_fraction.value,
    };
    let environment = EnvironmentFrameInput {
        lens_flare_intensity: gui.lens_flare_intensity.value,
        lens_flare_sun_pixel_scale: gui.lens_flare_sun_pixel_scale.value,
        clouds: CloudGuiParams {
            // Disabled for now; infrastructure kept for easy re-enable.
            enabled: false,
            coverage: gui.cloud_coverage.value,
            density: gui.cloud_density.value,
            bottom_height: gui.cloud_bottom_height.value,
            top_height: gui.cloud_top_height.value,
            shape_scale: gui.cloud_shape_scale.value,
            detail_scale: gui.cloud_detail_scale.value,
            detail_strength: gui.cloud_detail_strength.value,
            wind_speed: gui.cloud_wind_speed.value,
            primary_steps: gui.cloud_primary_steps.value,
            light_steps: gui.cloud_light_steps.value,
            temporal_alpha: gui.cloud_temporal_alpha.value,
            absorption: gui.cloud_absorption.value,
            phase_eccentricity: gui.cloud_phase_eccentricity.value,
            silver_intensity: gui.cloud_silver_intensity.value,
            max_distance: gui.cloud_max_distance.value,
            // Disabled for now; restore the original expression to re-enable.
            shadows_enabled: false,
            shadow_strength: gui.cloud_shadow_strength.value,
            shadow_min_transmittance: gui.cloud_shadow_min_transmittance.value,
            shadow_steps: gui.cloud_shadow_steps.value,
        },
        sun: SunFrameInput {
            direction: live.sun_direction,
            size: gui.sun_size.value,
            color: color_to_vec3(gui.sun_color.value),
            luminance: gui.sun_luminance.value,
            display_luminance: gui.sun_display_luminance.value,
            altitude: live.sun_altitude,
            azimuth: live.sun_azimuth,
        },
        god_rays: GodRayFrameInput {
            max_depth: gui.god_ray_max_depth.value,
            max_checks: gui.god_ray_max_checks.value,
            temporal_blend_enabled: gui.god_ray_temporal_blend.value,
            temporal_alpha: gui.god_ray_temporal_alpha.value,
            weight: gui.god_ray_weight.value,
            color: color_to_vec3(gui.sun_color.value),
        },
        starlight: StarlightFrameInput {
            iterations: gui.starlight_iterations.value,
            formuparam: gui.starlight_formuparam.value,
            volsteps: gui.starlight_volsteps.value,
            stepsize: gui.starlight_stepsize.value,
            zoom: gui.starlight_zoom.value,
            tile: gui.starlight_tile.value,
            speed: gui.starlight_speed.value,
            brightness: gui.starlight_brightness.value,
            darkmatter: gui.starlight_darkmatter.value,
            distfading: gui.starlight_distfading.value,
            saturation: gui.starlight_saturation.value,
        },
    };

    FrozenRenderFrameInputs {
        terrain,
        materials,
        vegetation,
        wind,
        environment,
    }
}
