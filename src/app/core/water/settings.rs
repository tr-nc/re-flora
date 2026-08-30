use crate::app::GuiAdjustables;
use crate::WaterPlan;
use re_flora_water::PondWaterConfig;

#[derive(Clone, Debug)]
pub(in crate::app::core) struct WaterRuntimeOverrides {
    profile: Option<PondWaterConfig>,
    plan: WaterPlan,
}

impl WaterRuntimeOverrides {
    pub(in crate::app::core) fn from_plan(
        plan: WaterPlan,
        profile: Option<PondWaterConfig>,
    ) -> Self {
        Self { profile, plan }
    }

    pub(in crate::app::core) fn apply(&self, config: &mut PondWaterConfig) {
        if let Some(profile) = &self.profile {
            *config = profile.clone();
        }
        if let Some(particle_count) = self.plan.particles {
            *config = config.clone().with_particle_count(particle_count);
        }
        if let Some(edge_len) = self.plan.particle_edge_len {
            config.set_particle_edge_len(edge_len);
        }
        if let Some(grid_dim) = self.plan.grid {
            *config = config.clone().with_cubic_grid_dim(grid_dim);
        }
        if let Some(substep_hz) = self.plan.substep_hz {
            config.substep_dt = substep_hz.recip();
        }
        if let Some(margin_cells) = self.plan.terrain_margin_cells {
            config.terrain_collision_margin_cells = margin_cells;
        }
        if let Some(damping_per_sec) = self.plan.damping {
            config.linear_damping_per_sec = damping_per_sec;
        }
        if let Some(damping_per_sec) = self.plan.terrain_tangent_damping {
            config.terrain_tangent_damping_per_sec = damping_per_sec;
        }
        if let Some(stiffness) = self.plan.stiffness {
            config.stiffness = stiffness;
        }
        if let Some(gamma) = self.plan.gamma {
            config.gamma = gamma;
        }
        if let Some(j_min) = self.plan.j_min {
            config.j_min = j_min;
        }
    }
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

    #[test]
    fn runtime_overrides_do_not_mutate_persisted_desired_water_values() {
        let desired_damping = 0.25;
        let mut effective = PondWaterConfig::default().with_linear_damping_per_sec(desired_damping);
        let overrides = WaterRuntimeOverrides {
            profile: None,
            plan: WaterPlan {
                damping: Some(1.5),
                ..WaterPlan::default()
            },
        };

        overrides.apply(&mut effective);

        assert_eq!(desired_damping, 0.25);
        assert_eq!(effective.linear_damping_per_sec, 1.5);
    }

    #[test]
    fn runtime_owns_the_primary_water_plan_without_field_copying() {
        let plan = WaterPlan {
            damping: Some(1.5),
            ..WaterPlan::default()
        };
        let overrides = WaterRuntimeOverrides::from_plan(plan, None);

        assert_eq!(overrides.plan.damping, Some(1.5));
    }
}
