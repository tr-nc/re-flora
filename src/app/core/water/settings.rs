use crate::app::GuiAdjustables;
use crate::AppOptions;
use glam::{UVec3, Vec3};
use re_flora_water::PondWaterConfig;

pub(in crate::app::core) const EXPERIENCE_PARTICLE_COUNT: usize = 10_000;
const EXPERIENCE_SUBSTEP_HZ: f32 = 60.0;
pub(in crate::app::core) const EXPERIENCE_INITIAL_FLUID_MIN_WS: Vec3 = Vec3::new(0.48, 0.32, 0.48);
pub(in crate::app::core) const EXPERIENCE_INITIAL_FLUID_MAX_WS: Vec3 = Vec3::new(1.52, 0.72, 1.52);

#[derive(Clone, Debug, Default)]
pub(super) struct WaterRuntimeOverrides {
    profile: Option<PondWaterConfig>,
    particle_count: Option<usize>,
    particle_edge_len: Option<f32>,
    grid_dim: Option<u32>,
    substep_hz: Option<f32>,
    terrain_margin_cells: Option<f32>,
    damping_per_sec: Option<f32>,
    terrain_tangent_damping_per_sec: Option<f32>,
    stiffness: Option<f32>,
    gamma: Option<f32>,
    j_min: Option<f32>,
}

impl WaterRuntimeOverrides {
    fn from_options(options: &AppOptions) -> Self {
        Self {
            profile: None,
            particle_count: options.water_particles,
            particle_edge_len: options.water_particle_edge_len,
            grid_dim: options.water_grid,
            substep_hz: options.water_substep_hz,
            terrain_margin_cells: options.water_terrain_margin_cells,
            damping_per_sec: options.water_damping,
            terrain_tangent_damping_per_sec: options.water_terrain_tangent_damping,
            stiffness: options.water_stiffness,
            gamma: options.water_gamma,
            j_min: options.water_j_min,
        }
    }

    pub(super) fn apply(&self, config: &mut PondWaterConfig) {
        if let Some(profile) = &self.profile {
            *config = profile.clone();
        }
        if let Some(particle_count) = self.particle_count {
            *config = config.clone().with_particle_count(particle_count);
        }
        if let Some(edge_len) = self.particle_edge_len {
            config.set_particle_edge_len(edge_len);
        }
        if let Some(grid_dim) = self.grid_dim {
            *config = config.clone().with_cubic_grid_dim(grid_dim);
        }
        if let Some(substep_hz) = self.substep_hz {
            config.substep_dt = substep_hz.recip();
        }
        if let Some(margin_cells) = self.terrain_margin_cells {
            config.terrain_collision_margin_cells = margin_cells;
        }
        if let Some(damping_per_sec) = self.damping_per_sec {
            config.linear_damping_per_sec = damping_per_sec;
        }
        if let Some(damping_per_sec) = self.terrain_tangent_damping_per_sec {
            config.terrain_tangent_damping_per_sec = damping_per_sec;
        }
        if let Some(stiffness) = self.stiffness {
            config.stiffness = stiffness;
        }
        if let Some(gamma) = self.gamma {
            config.gamma = gamma;
        }
        if let Some(j_min) = self.j_min {
            config.j_min = j_min;
        }
    }
}

/// The typed facts needed to select one effective water configuration.
///
/// `AppOptions` is translated here and never crosses the `WaterRuntime` seam. The priority rule is
/// deliberately executable in one place: base world config, then named profile or persisted GUI,
/// then the deterministic experience, and finally explicit CLI overrides.
pub(in crate::app::core) struct WaterLaunchRequest {
    profile: Option<crate::WaterProfilePreference>,
    experience: bool,
    base: PondWaterConfig,
    persisted_gui_effective: PondWaterConfig,
    cells_per_unit: f32,
    overrides: WaterRuntimeOverrides,
}

#[derive(Clone, Debug)]
pub(super) struct ResolvedWaterLaunch {
    pub(super) effective: PondWaterConfig,
    pub(super) runtime_overrides: WaterRuntimeOverrides,
    pub(super) profile: Option<crate::WaterProfilePreference>,
    pub(super) experience: bool,
    pub(super) gui_config_applied: bool,
    pub(super) cells_per_unit: f32,
}

impl WaterLaunchRequest {
    pub(in crate::app::core) fn from_options(
        options: &AppOptions,
        persisted_gui: &GuiAdjustables,
        world_extent: Vec3,
        cells_per_unit: f32,
    ) -> Self {
        Self::from_launch_facts(
            options.water_profile,
            options.water_experience,
            persisted_gui,
            world_extent,
            cells_per_unit,
            WaterRuntimeOverrides::from_options(options),
        )
    }

    fn from_launch_facts(
        profile: Option<crate::WaterProfilePreference>,
        experience: bool,
        persisted_gui: &GuiAdjustables,
        world_extent: Vec3,
        cells_per_unit: f32,
        overrides: WaterRuntimeOverrides,
    ) -> Self {
        let world_grid_dim = UVec3::new(
            (world_extent.x * cells_per_unit).ceil() as u32,
            (world_extent.y * cells_per_unit).ceil() as u32,
            (world_extent.z * cells_per_unit).ceil() as u32,
        );
        let base = PondWaterConfig::default()
            .with_collider_bounds(Vec3::ZERO, world_extent)
            .with_grid_dim(world_grid_dim);
        let mut persisted_gui_effective = base.clone();
        apply_water_gui_adjustables_to_config(&mut persisted_gui_effective, persisted_gui);
        Self {
            profile,
            experience,
            base,
            persisted_gui_effective,
            cells_per_unit,
            overrides,
        }
    }

    pub(super) fn resolve(mut self) -> ResolvedWaterLaunch {
        let mut effective = self.base;

        if matches!(
            self.profile,
            Some(crate::WaterProfilePreference::Performance)
        ) {
            effective = effective
                .with_substep_hz(60.0)
                .with_terrain_collision_margin_cells(0.0)
                .with_linear_damping_per_sec(1.5);
        }

        let gui_config_applied = self.profile.is_none() && !self.experience;
        if gui_config_applied {
            effective = self.persisted_gui_effective;
        }
        if self.experience {
            apply_water_experience_config(&mut effective);
        }

        // A selected profile is also the live baseline: later GUI changes cannot silently replace
        // an explicit launch mode. Explicit overrides are replayed after every GUI observation.
        self.overrides.profile =
            (self.profile.is_some() || self.experience).then(|| effective.clone());
        self.overrides.apply(&mut effective);

        ResolvedWaterLaunch {
            effective,
            runtime_overrides: self.overrides,
            profile: self.profile,
            experience: self.experience,
            gui_config_applied,
            cells_per_unit: self.cells_per_unit,
        }
    }
}

pub(super) fn apply_water_experience_config(config: &mut PondWaterConfig) {
    *config = config
        .clone()
        .with_particle_count(EXPERIENCE_PARTICLE_COUNT)
        .with_initial_fluid_bounds(
            EXPERIENCE_INITIAL_FLUID_MIN_WS,
            EXPERIENCE_INITIAL_FLUID_MAX_WS,
        )
        .with_substep_hz(EXPERIENCE_SUBSTEP_HZ)
        .with_terrain_collision_margin_cells(0.0)
        .with_linear_damping_per_sec(1.5);
}

pub(in crate::app::core) fn apply_water_gui_adjustables_to_config(
    config: &mut PondWaterConfig,
    gui_adjustables: &GuiAdjustables,
) {
    let substep_hz = finite_at_least(
        gui_adjustables.water_substep_hz.value,
        1.0,
        config.substep_dt.recip(),
    );
    config.substep_dt = substep_hz.recip();
    let particle_edge_len = finite_at_least(
        gui_adjustables.water_particle_edge_len.value,
        1.0e-6,
        config.particle_volume.cbrt(),
    );
    config.set_particle_edge_len(particle_edge_len);
    config.terrain_collision_margin_cells = finite_at_least(
        gui_adjustables.water_terrain_margin_cells.value,
        0.0,
        config.terrain_collision_margin_cells,
    );
    config.terrain_density_min_fluid_fraction = finite_clamped(
        gui_adjustables
            .water_boundary_density_min_fluid_fraction
            .value,
        1.0e-3,
        1.0,
        config.terrain_density_min_fluid_fraction,
    );
    config.terrain_density_max_correction_factor = finite_at_least(
        gui_adjustables
            .water_boundary_density_max_correction_factor
            .value,
        1.0,
        config.terrain_density_max_correction_factor,
    );
    config.terrain_density_occupancy_transition_cells = finite_at_least(
        gui_adjustables
            .water_boundary_density_occupancy_transition_cells
            .value,
        1.0e-3,
        config.terrain_density_occupancy_transition_cells,
    );
    config.linear_damping_per_sec = finite_at_least(
        gui_adjustables.water_damping.value,
        0.0,
        config.linear_damping_per_sec,
    );
    config.quiet_settling_velocity_damping_per_sec = finite_at_least(
        gui_adjustables.water_quiet_settling_velocity_damping.value,
        0.0,
        config.quiet_settling_velocity_damping_per_sec,
    );
    config.quiet_settling_affine_damping_per_sec = finite_at_least(
        gui_adjustables.water_quiet_settling_affine_damping.value,
        0.0,
        config.quiet_settling_affine_damping_per_sec,
    );
    config.debug_spawn_height_offset = finite_at_least(
        gui_adjustables.water_debug_spawn_height_offset.value,
        0.0,
        config.debug_spawn_height_offset,
    );
    config.terrain_tangent_damping_per_sec = finite_at_least(
        gui_adjustables.water_terrain_tangent_damping.value,
        0.0,
        config.terrain_tangent_damping_per_sec,
    );
    config.gravity.y = finite_or(gui_adjustables.water_gravity_y.value, config.gravity.y);
    config.stiffness =
        finite_at_least(gui_adjustables.water_stiffness.value, 0.0, config.stiffness);
    config.gamma = finite_at_least(gui_adjustables.water_gamma.value, 0.0, config.gamma);
    config.j_min = finite_clamped(gui_adjustables.water_j_min.value, 1.0e-4, 1.0, config.j_min);
    config.wall_damping = finite_clamped(
        gui_adjustables.water_wall_damping.value,
        0.0,
        1.0,
        config.wall_damping,
    );
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn finite_at_least(value: f32, min: f32, fallback: f32) -> f32 {
    finite_or(value, fallback).max(min)
}

fn finite_clamped(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    finite_or(value, fallback).clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use re_flora_water::collider::WaterBoxCollider;

    fn request(
        profile: Option<crate::WaterProfilePreference>,
        experience: bool,
        gui: GuiAdjustables,
        overrides: WaterRuntimeOverrides,
    ) -> WaterLaunchRequest {
        WaterLaunchRequest::from_launch_facts(
            profile,
            experience,
            &gui,
            Vec3::splat(2.0),
            32.0,
            overrides,
        )
    }

    fn no_overrides() -> WaterRuntimeOverrides {
        WaterRuntimeOverrides::default()
    }

    #[test]
    fn implicit_launch_applies_persisted_gui_water_config() {
        let mut gui = GuiAdjustables::default();
        gui.water_damping.value = 0.25;

        let resolved = request(None, false, gui, no_overrides()).resolve();

        assert!(resolved.gui_config_applied);
        assert_eq!(resolved.effective.linear_damping_per_sec, 0.25);
        assert_eq!(resolved.effective.collider.max_ws, Vec3::splat(2.0));
        assert_eq!(resolved.effective.grid_dim, glam::UVec3::splat(64));
    }

    #[test]
    fn named_profile_replaces_persisted_gui_water_config() {
        let mut gui = GuiAdjustables::default();
        gui.water_damping.value = 0.25;

        let resolved = request(
            Some(crate::WaterProfilePreference::Performance),
            false,
            gui,
            no_overrides(),
        )
        .resolve();

        assert!(!resolved.gui_config_applied);
        assert_eq!(resolved.effective.substep_dt, 60.0_f32.recip());
        assert_eq!(resolved.effective.terrain_collision_margin_cells, 0.0);
        assert_eq!(resolved.effective.linear_damping_per_sec, 1.5);
    }

    #[test]
    fn explicit_default_profile_also_ignores_persisted_gui_water_config() {
        let mut gui = GuiAdjustables::default();
        gui.water_damping.value = 0.25;

        let resolved = request(
            Some(crate::WaterProfilePreference::Default),
            false,
            gui,
            no_overrides(),
        )
        .resolve();

        assert!(!resolved.gui_config_applied);
        assert_eq!(
            resolved.effective.linear_damping_per_sec,
            PondWaterConfig::default().linear_damping_per_sec
        );
    }

    #[test]
    fn experience_launch_is_deterministic() {
        let mut gui = GuiAdjustables::default();
        gui.water_damping.value = 0.25;

        let resolved = request(None, true, gui, no_overrides()).resolve();

        assert!(!resolved.gui_config_applied);
        assert_eq!(resolved.effective.particle_count, 10_000);
        assert_eq!(resolved.effective.substep_dt, 60.0_f32.recip());
        assert_eq!(resolved.effective.terrain_collision_margin_cells, 0.0);
        assert_eq!(resolved.effective.linear_damping_per_sec, 1.5);
        assert_eq!(
            resolved.effective.initial_fluid_bounds,
            Some(WaterBoxCollider::new(
                Vec3::new(0.48, 0.32, 0.48),
                Vec3::new(1.52, 0.72, 1.52),
            ))
        );
        assert!(resolved.effective.initial_fluid_bounds.unwrap().max_ws.y < 2.0);
    }

    #[test]
    fn explicit_override_wins_over_profile_and_experience() {
        let overrides = WaterRuntimeOverrides {
            particle_count: Some(1234),
            damping_per_sec: Some(0.75),
            ..no_overrides()
        };

        let resolved = request(
            Some(crate::WaterProfilePreference::Performance),
            true,
            GuiAdjustables::default(),
            overrides,
        )
        .resolve();

        assert_eq!(resolved.effective.particle_count, 1234);
        assert_eq!(resolved.effective.linear_damping_per_sec, 0.75);
    }

    #[test]
    fn runtime_overrides_do_not_mutate_persisted_desired_water_values() {
        let desired_damping = 0.25;
        let mut effective = PondWaterConfig::default().with_linear_damping_per_sec(desired_damping);
        let overrides = WaterRuntimeOverrides {
            damping_per_sec: Some(1.5),
            ..WaterRuntimeOverrides::default()
        };

        overrides.apply(&mut effective);

        assert_eq!(desired_damping, 0.25);
        assert_eq!(effective.linear_damping_per_sec, 1.5);
    }
}
