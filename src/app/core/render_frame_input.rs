use super::{FLORA_FULL_GROWTH_TICKS, FLORA_SPROUT_DELAY_TICKS};
use crate::app::{DebugSettings, GuiAdjustables};
use crate::tracer::{
    CloudGuiParams, EnvironmentFrameInput, FloraAppearanceFrameInput, FloraGrowthFrameInput,
    FloraMotionFrameInput, FruitMotionParams, GlassGuiParams, GodRayFrameInput, KochiaMotionParams,
    KochiaVisualParams, LeafLightingFrameInput, MaterialFrameInput, RenderFrameInputs,
    StarlightFrameInput, SunFrameInput, TerrainEditPreviewShape, TerrainFrameInput,
    VegetationFrameInput, WindFrameInput, WindGuiParams,
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
) -> RenderFrameInputs {
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
            inertial_response_enabled: gui.flora_inertial_response.value,
            response_controls: [
                gui.vegetation_response_speed.value,
                gui.vegetation_response_damping.value,
                gui.vegetation_response_gain.value,
                0.,
            ],
            response_pose_hz: gui.vegetation_response_pose_hz.value,
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

    RenderFrameInputs {
        terrain,
        materials,
        vegetation,
        wind,
        environment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::gui_config::WindSourceGuiValues;
    use crate::wind::WindSource;

    #[test]
    fn mapper_preserves_every_renderer_fact_and_conversion() {
        let mut settings = DebugSettings::load();
        let gui = &mut settings.adjustables;
        let mut float_sentinel = 10.0_f32;
        let mut uint_sentinel = 100_u32;
        let mut int_sentinel = -100_i32;
        let mut color_sentinel = 10_u8;

        macro_rules! float {
            ($field:ident) => {{
                float_sentinel += 0.25;
                gui.$field.value = float_sentinel;
                float_sentinel
            }};
        }
        macro_rules! uint {
            ($field:ident) => {{
                uint_sentinel += 1;
                gui.$field.value = uint_sentinel;
                uint_sentinel
            }};
        }
        macro_rules! int {
            ($field:ident) => {{
                int_sentinel += 1;
                gui.$field.value = int_sentinel;
                int_sentinel
            }};
        }
        macro_rules! color {
            ($field:ident) => {{
                let value =
                    Color32::from_rgb(color_sentinel, color_sentinel + 1, color_sentinel + 2);
                color_sentinel += 3;
                gui.$field.value = value;
                color_to_vec3(value)
            }};
        }

        gui.flora_growth_override_enabled.value = true;
        gui.glass_per_voxel_reflection.value = true;

        let terrain_ray_origin_offset_world = float!(terrain_ray_origin_offset_world);
        let ddgi_receiver_visibility_bias_world = float!(ddgi_receiver_visibility_bias_world);
        let ddgi_history_retention = float!(ddgi_history_retention);
        let terrain_self_shadow_tolerance_voxels = float!(terrain_self_shadow_tolerance_voxels);
        let glass_tint = color!(glass_tint);
        let glass_reflection_strength = float!(glass_reflection_strength);
        let glass_ssr_strength = float!(glass_ssr_strength);
        let glass_ssr_steps = uint!(glass_ssr_steps);
        let glass_ssr_min_hit_thickness_voxels = float!(glass_ssr_min_hit_thickness_voxels);
        let glass_ssr_footprint_pixels = float!(glass_ssr_footprint_pixels);
        let glass_refraction_strength = float!(glass_refraction_strength);
        let glass_alpha = float!(glass_alpha);
        let glass_glint_strength = float!(glass_glint_strength);
        let voxel_dirt_color = color!(voxel_dirt_color);
        let voxel_sand_color = color!(voxel_sand_color);
        let voxel_cherry_wood_color = color!(voxel_cherry_wood_color);
        let voxel_oak_wood_color = color!(voxel_oak_wood_color);
        let voxel_rock_color = color!(voxel_rock_color);
        let flora_growth_override = float!(flora_growth_override);
        let flora_instance_hue_offset = float!(flora_instance_hue_offset);
        let flora_instance_saturation_offset = float!(flora_instance_saturation_offset);
        let flora_instance_value_offset = float!(flora_instance_value_offset);
        let flora_voxel_hue_offset = float!(flora_voxel_hue_offset);
        let flora_voxel_saturation_offset = float!(flora_voxel_saturation_offset);
        let flora_voxel_value_offset = float!(flora_voxel_value_offset);
        let grass_bottom_dark = color!(grass_bottom_dark_color);
        let grass_bottom_light = color!(grass_bottom_light_color);
        let grass_tip_dark = color!(grass_tip_dark_color);
        let grass_tip_light = color!(grass_tip_light_color);
        let kochia_bottom_darkening = float!(kochia_bottom_darkening);
        let kochia_branch_value_variation = float!(kochia_branch_value_variation);
        let kochia_voxel_value_variation = float!(kochia_voxel_value_variation);
        let kochia_branch_count = uint!(kochia_branch_count);
        let kochia_bottom_diameter_voxels = float!(kochia_bottom_diameter_voxels);
        let kochia_waist_diameter_voxels = float!(kochia_waist_diameter_voxels);
        let kochia_top_diameter_voxels = float!(kochia_top_diameter_voxels);
        let kochia_waist_height = float!(kochia_waist_height);
        let grass_vibration_amplitude_voxels = float!(grass_vibration_amplitude_voxels);
        let grass_vibration_primary_speed = float!(grass_vibration_primary_speed);
        let grass_vibration_secondary_speed = float!(grass_vibration_secondary_speed);
        let grass_natural_bend_min_voxels = float!(grass_natural_bend_min_voxels);
        let grass_natural_bend_max_voxels = float!(grass_natural_bend_max_voxels);
        let flora_bend_height_power = float!(flora_bend_height_power);
        let kochia_body_wind_response = float!(kochia_body_wind_response);
        let kochia_branch_jelly_amplitude_voxels = float!(kochia_branch_jelly_amplitude_voxels);
        let kochia_branch_jelly_speed = float!(kochia_branch_jelly_speed);
        let kochia_branch_phase_spread = float!(kochia_branch_phase_spread);
        let kochia_tip_flutter_amplitude_voxels = float!(kochia_tip_flutter_amplitude_voxels);
        let kochia_tip_flutter_speed = float!(kochia_tip_flutter_speed);
        let leaf_paddle_amplitude_voxels = float!(leaf_paddle_amplitude_voxels);
        let leaf_paddle_primary_speed = float!(leaf_paddle_primary_speed);
        let leaf_paddle_secondary_speed = float!(leaf_paddle_secondary_speed);
        let leaf_paddle_amplitude_wind_start_strength =
            float!(leaf_paddle_amplitude_wind_start_strength);
        let leaf_paddle_amplitude_wind_full_strength =
            float!(leaf_paddle_amplitude_wind_full_strength);
        let leaf_paddle_amplitude_wind_knee_bias = float!(leaf_paddle_amplitude_wind_knee_bias);
        let leaf_paddle_frequency_wind_start_strength =
            float!(leaf_paddle_frequency_wind_start_strength);
        let leaf_paddle_frequency_wind_full_strength =
            float!(leaf_paddle_frequency_wind_full_strength);
        let leaf_paddle_frequency_wind_knee_bias = float!(leaf_paddle_frequency_wind_knee_bias);
        let leaf_paddle_frequency_min_multiplier = float!(leaf_paddle_frequency_min_multiplier);
        let leaf_paddle_frequency_max_multiplier = float!(leaf_paddle_frequency_max_multiplier);
        let leaf_shadow_fragment_opacity = float!(leaf_shadow_fragment_opacity);
        let leaf_shadow_strength = float!(leaf_shadow_strength);
        let leaf_shadow_min_transmittance = float!(leaf_shadow_min_transmittance);
        let leaf_shadow_filter_radius_texels = float!(leaf_shadow_filter_radius_texels);
        let leaf_transmission_strength = float!(leaf_transmission_strength);
        let flora_spawn_duration_seconds = float!(flora_spawn_duration_seconds);
        let flora_spawn_rise_fraction = float!(flora_spawn_rise_fraction);
        let flora_spawn_overshoot_min_voxels = float!(flora_spawn_overshoot_min_voxels);
        let flora_spawn_overshoot_max_voxels = float!(flora_spawn_overshoot_max_voxels);
        let flora_spawn_stagger_seconds = float!(flora_spawn_stagger_seconds);
        let wind_directional_bias_fraction = float!(wind_directional_bias_fraction);
        let wind_turbulence_fraction = float!(wind_turbulence_fraction);
        let lens_flare_intensity = float!(lens_flare_intensity);
        let lens_flare_sun_pixel_scale = float!(lens_flare_sun_pixel_scale);
        let cloud_coverage = float!(cloud_coverage);
        let cloud_density = float!(cloud_density);
        let cloud_bottom_height = float!(cloud_bottom_height);
        let cloud_top_height = float!(cloud_top_height);
        let cloud_shape_scale = float!(cloud_shape_scale);
        let cloud_detail_scale = float!(cloud_detail_scale);
        let cloud_detail_strength = float!(cloud_detail_strength);
        let cloud_wind_speed = float!(cloud_wind_speed);
        let cloud_primary_steps = uint!(cloud_primary_steps);
        let cloud_light_steps = uint!(cloud_light_steps);
        let cloud_temporal_alpha = float!(cloud_temporal_alpha);
        let cloud_absorption = float!(cloud_absorption);
        let cloud_phase_eccentricity = float!(cloud_phase_eccentricity);
        let cloud_silver_intensity = float!(cloud_silver_intensity);
        let cloud_max_distance = float!(cloud_max_distance);
        let cloud_shadow_strength = float!(cloud_shadow_strength);
        let cloud_shadow_min_transmittance = float!(cloud_shadow_min_transmittance);
        let cloud_shadow_steps = uint!(cloud_shadow_steps);
        let sun_size = float!(sun_size);
        let sun_color = color!(sun_color);
        let sun_luminance = float!(sun_luminance);
        let sun_display_luminance = float!(sun_display_luminance);
        let god_ray_max_depth = float!(god_ray_max_depth);
        let god_ray_max_checks = uint!(god_ray_max_checks);
        gui.god_ray_temporal_blend.value = true;
        let god_ray_temporal_alpha = float!(god_ray_temporal_alpha);
        let god_ray_weight = float!(god_ray_weight);
        let starlight_iterations = int!(starlight_iterations);
        let starlight_formuparam = float!(starlight_formuparam);
        let starlight_volsteps = int!(starlight_volsteps);
        let starlight_stepsize = float!(starlight_stepsize);
        let starlight_zoom = float!(starlight_zoom);
        let starlight_tile = float!(starlight_tile);
        let starlight_speed = float!(starlight_speed);
        let starlight_brightness = float!(starlight_brightness);
        let starlight_darkmatter = float!(starlight_darkmatter);
        let starlight_distfading = float!(starlight_distfading);
        let starlight_saturation = float!(starlight_saturation);
        let _ = color_sentinel;

        settings.tree.desc.fruit_swing_length_voxels = 201.25;
        settings.tree.desc.fruit_swing_max_angle_degrees = 202.25;
        settings.tree.desc.fruit_swing_speed = 203.25;
        settings.tree.desc.fruit_swing_speed_variation = 204.25;
        settings.tree.desc.fruit_swing_min_response = 205.25;
        let wind_source = WindSource::new(211.0, 212.0, 213.0, 214, 215.0, 216.0, 217.0);
        settings.wind_sources = vec![WindSourceGuiValues {
            name: "sentinel".to_owned(),
            muted: false,
            source: wind_source,
        }];

        let live = LiveRenderFrameFacts {
            world_tick_seconds: 301.25,
            flora_tick: 302,
            visual_time_since_start: 303.125,
            sun_direction: Vec3::new(304.0, 305.0, 306.0),
            sun_altitude: 307.25,
            sun_azimuth: 308.25,
            terrain_edit_preview_center: Some(Vec3::new(309.0, 310.0, 311.0)),
            terrain_edit_preview_radius: 312.25,
            terrain_edit_preview_shape: TerrainEditPreviewShape::SurfaceCircle,
            terrain_edit_preview_color: Vec3::new(313.0, 314.0, 315.0),
            terrain_edit_preview_alpha: 316.25,
        };
        let expected = RenderFrameInputs {
            terrain: TerrainFrameInput {
                ray_origin_offset_world: terrain_ray_origin_offset_world,
                ddgi_receiver_visibility_bias_world,
                ddgi_history_retention,
                self_shadow_tolerance_voxels: terrain_self_shadow_tolerance_voxels,
                edit_preview_center: live.terrain_edit_preview_center,
                edit_preview_radius: live.terrain_edit_preview_radius,
                edit_preview_shape: live.terrain_edit_preview_shape,
                edit_preview_color: live.terrain_edit_preview_color,
                edit_preview_alpha: live.terrain_edit_preview_alpha,
            },
            materials: MaterialFrameInput {
                glass: GlassGuiParams {
                    tint: glass_tint,
                    reflection_strength: glass_reflection_strength,
                    ssr_strength: glass_ssr_strength,
                    ssr_steps: glass_ssr_steps,
                    per_voxel_reflection: true,
                    ssr_min_hit_thickness_voxels: glass_ssr_min_hit_thickness_voxels,
                    ssr_footprint_pixels: glass_ssr_footprint_pixels,
                    refraction_strength: glass_refraction_strength,
                    alpha: glass_alpha,
                    glint_strength: glass_glint_strength,
                },
                voxel_dirt_color,
                voxel_sand_color,
                voxel_cherry_wood_color,
                voxel_oak_wood_color,
                voxel_rock_color,
            },
            vegetation: VegetationFrameInput {
                appearance: FloraAppearanceFrameInput {
                    growth_override_enabled: true,
                    growth_override: flora_growth_override,
                    instance_hsv_offset_max: Vec3::new(
                        flora_instance_hue_offset,
                        flora_instance_saturation_offset,
                        flora_instance_value_offset,
                    ),
                    voxel_hsv_offset_max: Vec3::new(
                        flora_voxel_hue_offset,
                        flora_voxel_saturation_offset,
                        flora_voxel_value_offset,
                    ),
                    grass_bottom_dark,
                    grass_bottom_light,
                    grass_tip_dark,
                    grass_tip_light,
                    kochia: KochiaVisualParams {
                        bottom_darkening: kochia_bottom_darkening,
                        branch_value_variation: kochia_branch_value_variation,
                        voxel_value_variation: kochia_voxel_value_variation,
                        branch_count: kochia_branch_count,
                        bottom_diameter_voxels: kochia_bottom_diameter_voxels,
                        waist_diameter_voxels: kochia_waist_diameter_voxels,
                        top_diameter_voxels: kochia_top_diameter_voxels,
                        waist_height: kochia_waist_height,
                    },
                },
                motion: FloraMotionFrameInput {
                    inertial_response_enabled: settings.adjustables.flora_inertial_response.value,
                    response_controls: [
                        settings.adjustables.vegetation_response_speed.value,
                        settings.adjustables.vegetation_response_damping.value,
                        settings.adjustables.vegetation_response_gain.value,
                        0.,
                    ],
                    response_pose_hz: settings.adjustables.vegetation_response_pose_hz.value,
                    world_tick_seconds: live.world_tick_seconds,
                    grass_vibration_amplitude_voxels,
                    grass_vibration_primary_speed,
                    grass_vibration_secondary_speed,
                    grass_natural_bend_min_voxels,
                    grass_natural_bend_max_voxels,
                    bend_height_power: flora_bend_height_power,
                    kochia: KochiaMotionParams {
                        body_wind_response: kochia_body_wind_response,
                        branch_jelly_amplitude_voxels: kochia_branch_jelly_amplitude_voxels,
                        branch_jelly_speed: kochia_branch_jelly_speed,
                        branch_phase_spread: kochia_branch_phase_spread,
                        tip_flutter_amplitude_voxels: kochia_tip_flutter_amplitude_voxels,
                        tip_flutter_speed: kochia_tip_flutter_speed,
                    },
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
                    fruit: FruitMotionParams {
                        swing_length_voxels: 201.25,
                        max_angle_radians: 202.25_f32.to_radians(),
                        swing_speed: 203.25,
                        speed_variation: 204.25,
                        min_response: 205.25,
                    },
                },
                leaf_lighting: LeafLightingFrameInput {
                    shadow_fragment_opacity: leaf_shadow_fragment_opacity,
                    shadow_strength: leaf_shadow_strength,
                    shadow_min_transmittance: leaf_shadow_min_transmittance,
                    shadow_filter_radius_texels: leaf_shadow_filter_radius_texels,
                    transmission_strength: leaf_transmission_strength,
                },
                growth: FloraGrowthFrameInput {
                    flora_tick: live.flora_tick,
                    sprout_delay_ticks: FLORA_SPROUT_DELAY_TICKS,
                    full_growth_ticks: FLORA_FULL_GROWTH_TICKS,
                    spawn_time_ms: 303_125,
                    spawn_duration_seconds: flora_spawn_duration_seconds,
                    spawn_rise_fraction: flora_spawn_rise_fraction,
                    spawn_overshoot_min_voxels: flora_spawn_overshoot_min_voxels,
                    spawn_overshoot_max_voxels: flora_spawn_overshoot_max_voxels,
                    spawn_stagger_seconds: flora_spawn_stagger_seconds,
                },
            },
            wind: WindFrameInput {
                sources: WindGuiParams {
                    sources: vec![wind_source],
                },
                directional_bias_fraction: wind_directional_bias_fraction,
                turbulence_fraction: wind_turbulence_fraction,
            },
            environment: EnvironmentFrameInput {
                lens_flare_intensity,
                lens_flare_sun_pixel_scale,
                clouds: CloudGuiParams {
                    enabled: false,
                    coverage: cloud_coverage,
                    density: cloud_density,
                    bottom_height: cloud_bottom_height,
                    top_height: cloud_top_height,
                    shape_scale: cloud_shape_scale,
                    detail_scale: cloud_detail_scale,
                    detail_strength: cloud_detail_strength,
                    wind_speed: cloud_wind_speed,
                    primary_steps: cloud_primary_steps,
                    light_steps: cloud_light_steps,
                    temporal_alpha: cloud_temporal_alpha,
                    absorption: cloud_absorption,
                    phase_eccentricity: cloud_phase_eccentricity,
                    silver_intensity: cloud_silver_intensity,
                    max_distance: cloud_max_distance,
                    shadows_enabled: false,
                    shadow_strength: cloud_shadow_strength,
                    shadow_min_transmittance: cloud_shadow_min_transmittance,
                    shadow_steps: cloud_shadow_steps,
                },
                sun: SunFrameInput {
                    direction: live.sun_direction,
                    size: sun_size,
                    color: sun_color,
                    luminance: sun_luminance,
                    display_luminance: sun_display_luminance,
                    altitude: live.sun_altitude,
                    azimuth: live.sun_azimuth,
                },
                god_rays: GodRayFrameInput {
                    max_depth: god_ray_max_depth,
                    max_checks: god_ray_max_checks,
                    temporal_blend_enabled: true,
                    temporal_alpha: god_ray_temporal_alpha,
                    weight: god_ray_weight,
                    color: sun_color,
                },
                starlight: StarlightFrameInput {
                    iterations: starlight_iterations,
                    formuparam: starlight_formuparam,
                    volsteps: starlight_volsteps,
                    stepsize: starlight_stepsize,
                    zoom: starlight_zoom,
                    tile: starlight_tile,
                    speed: starlight_speed,
                    brightness: starlight_brightness,
                    darkmatter: starlight_darkmatter,
                    distfading: starlight_distfading,
                    saturation: starlight_saturation,
                },
            },
        };

        let actual = freeze_render_frame_inputs(&settings, live);
        assert_eq!(actual, expected);
    }
}
