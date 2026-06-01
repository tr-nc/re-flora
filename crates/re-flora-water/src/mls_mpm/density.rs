use glam::{IVec3, Mat3, UVec3, Vec3};

use super::{
    transfer::{grid_index_dims, in_grid},
    WaterGridNode, WaterTerrainGridSample,
};

pub(super) const MAX_J: f32 = 8.0;
pub(super) const NO_TENSION_MAX_J: f32 = 1.0;
const MIN_FLUID_DENSITY: f32 = 1.0e-8;
#[cfg(test)]
const MAX_J_LOG_STEP_PER_SUBSTEP: f32 = 0.10;
// Blend a small MLS grid-density estimate into the deformation-history J each
// substep. Pure velocity-gradient J can relax to rest volume after wall/terrain
// collision projection even when particles are visibly overpacked; this feedback
// re-anchors pressure to the configured marker volume without a neighbor solve.
#[cfg(test)]
const DENSITY_J_FEEDBACK_PER_SECOND: f32 = 12.64;
// Ignore tiny density-estimate compression. The MLS kernel/marker discretization
// constantly produces sub-percent local density noise; feeding that straight into
// pressure keeps quiet puddles breathing forever.
const DENSITY_J_FEEDBACK_DEADBAND: f32 = 0.02;
// Fill missing density-kernel support when terrain SDF occupies part of a
// particle's pressure stencil. This is an mDBC/Ghost-SPH style density sample:
// solid-side stencil weight contributes virtual density extrapolated from the
// mirrored fluid side, with a hydrostatic pressure offset. It is used only for
// EOS pressure/stress; it does not add real grid mass or alter velocity
// normalization.
const TERRAIN_DENSITY_MIN_SOLID_WEIGHT: f32 = 1.0e-5;
const TERRAIN_GHOST_MIRROR_MIN_DISTANCE_CELLS: f32 = 0.25;

#[cfg(test)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub(super) fn particle_density_from_grid(
    grid: &[WaterGridNode],
    grid_dim: glam::UVec3,
    base: IVec3,
    wx: [f32; 3],
    wy: [f32; 3],
    wz: [f32; 3],
    inv_cell_volume: f32,
) -> Option<f32> {
    if grid.is_empty() || inv_cell_volume <= 0.0 || !inv_cell_volume.is_finite() {
        return None;
    }

    let mut gathered_mass = 0.0f32;
    for oz in 0..3 {
        for oy in 0..3 {
            for ox in 0..3 {
                let node = base + IVec3::new(ox, oy, oz);
                if !in_grid(node, grid_dim) {
                    continue;
                }

                let weight = wx[ox as usize] * wy[oy as usize] * wz[oz as usize];
                if weight <= 0.0 {
                    continue;
                }

                let node_idx = grid_index_dims(
                    grid_dim,
                    node.x as u32,
                    node.y as u32,
                    node.z as u32,
                );
                gathered_mass += grid[node_idx].mass * weight;
            }
        }
    }

    let density = gathered_mass * inv_cell_volume;
    (density.is_finite() && density > MIN_FLUID_DENSITY).then_some(density)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TerrainBoundaryDensitySample {
    pub(super) density: f32,
    pub(super) correction_factor: f32,
    pub(super) fluid_fraction: f32,
    pub(super) solid_weight: f32,
}

pub(super) fn terrain_boundary_density_correction(
    raw_density: f32,
    terrain_grid: &[WaterTerrainGridSample],
    terrain_ghost_density: &[f32],
    grid_dim: UVec3,
    base: IVec3,
    wx: [f32; 3],
    wy: [f32; 3],
    wz: [f32; 3],
    dx: f32,
    min_fluid_fraction: f32,
    max_correction_factor: f32,
    occupancy_transition_cells: f32,
) -> TerrainBoundaryDensitySample {
    if !raw_density.is_finite() || raw_density <= 0.0 {
        return TerrainBoundaryDensitySample {
            density: raw_density,
            correction_factor: 1.0,
            fluid_fraction: 1.0,
            solid_weight: 0.0,
        };
    }

    let ghost = terrain_ghost_density_contribution(
        terrain_grid,
        terrain_ghost_density,
        grid_dim,
        base,
        wx,
        wy,
        wz,
        dx,
        occupancy_transition_cells,
    );
    let solid_weight = ghost.solid_weight;
    if solid_weight <= TERRAIN_DENSITY_MIN_SOLID_WEIGHT {
        return TerrainBoundaryDensitySample {
            density: raw_density,
            correction_factor: 1.0,
            fluid_fraction: 1.0,
            solid_weight,
        };
    }

    let min_fluid_fraction = min_fluid_fraction.clamp(1.0e-3, 1.0);
    let fluid_fraction = (1.0 - solid_weight).clamp(min_fluid_fraction, 1.0);
    let max_correction_factor = max_correction_factor.max(1.0);
    let max_density = raw_density * max_correction_factor;
    let fallback_density = raw_density
        * fluid_fraction
            .recip()
            .min(max_correction_factor);
    let ghost_density = raw_density + ghost.weighted_density;
    let density = if ghost_density.is_finite() && ghost_density > raw_density {
        ghost_density.min(max_density).max(raw_density)
    } else {
        fallback_density
    };
    let density = if density.is_finite() && density > 0.0 {
        density
    } else {
        raw_density
    };
    TerrainBoundaryDensitySample {
        density,
        correction_factor: (density / raw_density).max(1.0),
        fluid_fraction,
        solid_weight,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct TerrainGhostDensityContribution {
    solid_weight: f32,
    weighted_density: f32,
}

fn terrain_ghost_density_contribution(
    terrain_grid: &[WaterTerrainGridSample],
    terrain_ghost_density: &[f32],
    grid_dim: UVec3,
    base: IVec3,
    wx: [f32; 3],
    wy: [f32; 3],
    wz: [f32; 3],
    dx: f32,
    occupancy_transition_cells: f32,
) -> TerrainGhostDensityContribution {
    if terrain_grid.is_empty() || dx <= 0.0 || !dx.is_finite() {
        return TerrainGhostDensityContribution::default();
    }

    let mut solid_weight = 0.0f32;
    let mut weighted_density = 0.0f32;
    for oz in 0..3 {
        for oy in 0..3 {
            for ox in 0..3 {
                let node = base + IVec3::new(ox, oy, oz);
                if !in_grid(node, grid_dim) {
                    continue;
                }

                let weight = wx[ox as usize] * wy[oy as usize] * wz[oz as usize];
                if weight <= 0.0 {
                    continue;
                }

                let node_idx = grid_index_dims(
                    grid_dim,
                    node.x as u32,
                    node.y as u32,
                    node.z as u32,
                );
                let Some(sample) = terrain_grid.get(node_idx) else {
                    continue;
                };
                if !sample.has_sdf {
                    continue;
                }

                let occupancy =
                    terrain_solid_occupancy_from_sdf(sample.sdf, dx, occupancy_transition_cells);
                if occupancy <= 0.0 {
                    continue;
                }
                solid_weight += weight * occupancy;

                let ghost_density = terrain_ghost_density.get(node_idx).copied().unwrap_or(0.0);
                if ghost_density.is_finite() && ghost_density > MIN_FLUID_DENSITY {
                    weighted_density += weight * occupancy * ghost_density;
                }
            }
        }
    }

    TerrainGhostDensityContribution {
        solid_weight: solid_weight.clamp(0.0, 1.0),
        weighted_density: weighted_density.max(0.0),
    }
}

pub(super) fn terrain_ghost_density_for_grid_node(
    grid: &[WaterGridNode],
    grid_dim: UVec3,
    node: UVec3,
    sample: WaterTerrainGridSample,
    dx: f32,
    inv_dx: f32,
    inv_cell_volume: f32,
    rest_density: f32,
    stiffness: f32,
    gamma: f32,
    pressure_floor: f32,
    gravity: Vec3,
    occupancy_transition_cells: f32,
) -> Option<f32> {
    if !sample.has_sdf
        || terrain_solid_occupancy_from_sdf(sample.sdf, dx, occupancy_transition_cells) <= 0.0
    {
        return None;
    }

    let normal = terrain_sample_normal(sample)?;
    let node_local = node.as_vec3() * dx;
    let surface_local = node_local - sample.sdf * normal;
    let mirror_distance = (-sample.sdf).max(dx * TERRAIN_GHOST_MIRROR_MIN_DISTANCE_CELLS.max(0.0));
    let ghost_local = surface_local - mirror_distance * normal;
    let mirror_local = surface_local + mirror_distance * normal;
    let mirror_density = fluid_density_at_local_position(
        grid,
        grid_dim,
        mirror_local,
        inv_dx,
        inv_cell_volume,
    )
    .unwrap_or(rest_density);
    if !mirror_density.is_finite() || mirror_density <= 0.0 {
        return None;
    }

    let mirror_pressure = fluid_eos_pressure(
        stiffness,
        gamma,
        mirror_density,
        rest_density,
        pressure_floor,
    )
    .max(0.0);
    let hydrostatic_delta = rest_density * gravity.dot(ghost_local - mirror_local);
    let ghost_pressure = (mirror_pressure + hydrostatic_delta).max(0.0);
    let ghost_density = fluid_density_from_eos_pressure(
        stiffness,
        gamma,
        ghost_pressure,
        rest_density,
    )
    .unwrap_or(mirror_density);
    (ghost_density.is_finite() && ghost_density > MIN_FLUID_DENSITY).then_some(ghost_density)
}

fn fluid_density_at_local_position(
    grid: &[WaterGridNode],
    grid_dim: UVec3,
    local_pos: Vec3,
    inv_dx: f32,
    inv_cell_volume: f32,
) -> Option<f32> {
    if !local_pos.is_finite() || inv_dx <= 0.0 || !inv_dx.is_finite() {
        return None;
    }

    let grid_pos = local_pos * inv_dx;
    let base = grid_pos.floor().as_ivec3();
    let frac = grid_pos - base.as_vec3();
    let wx = [1.0 - frac.x, frac.x];
    let wy = [1.0 - frac.y, frac.y];
    let wz = [1.0 - frac.z, frac.z];
    let mut gathered_mass = 0.0f32;
    for oz in 0..2 {
        for oy in 0..2 {
            for ox in 0..2 {
                let node = base + IVec3::new(ox, oy, oz);
                if !in_grid(node, grid_dim) {
                    continue;
                }

                let weight = wx[ox as usize] * wy[oy as usize] * wz[oz as usize];
                if weight <= 0.0 {
                    continue;
                }

                let node_idx = grid_index_dims(
                    grid_dim,
                    node.x as u32,
                    node.y as u32,
                    node.z as u32,
                );
                gathered_mass += grid[node_idx].mass * weight;
            }
        }
    }

    let density = gathered_mass * inv_cell_volume;
    (density.is_finite() && density > MIN_FLUID_DENSITY).then_some(density)
}

fn terrain_sample_normal(sample: WaterTerrainGridSample) -> Option<Vec3> {
    if !sample.normal.is_finite() {
        return None;
    }
    let len2 = sample.normal.length_squared();
    (len2 > 1.0e-8 && len2.is_finite()).then_some(sample.normal / len2.sqrt())
}

#[cfg(test)]
pub(super) fn terrain_solid_kernel_weight(
    terrain_grid: &[WaterTerrainGridSample],
    grid_dim: glam::UVec3,
    base: IVec3,
    wx: [f32; 3],
    wy: [f32; 3],
    wz: [f32; 3],
    dx: f32,
    occupancy_transition_cells: f32,
) -> f32 {
    if terrain_grid.is_empty() || dx <= 0.0 || !dx.is_finite() {
        return 0.0;
    }

    let mut solid_weight = 0.0f32;
    for oz in 0..3 {
        for oy in 0..3 {
            for ox in 0..3 {
                let node = base + IVec3::new(ox, oy, oz);
                if !in_grid(node, grid_dim) {
                    continue;
                }

                let weight = wx[ox as usize] * wy[oy as usize] * wz[oz as usize];
                if weight <= 0.0 {
                    continue;
                }

                let node_idx = grid_index_dims(
                    grid_dim,
                    node.x as u32,
                    node.y as u32,
                    node.z as u32,
                );
                let Some(sample) = terrain_grid.get(node_idx) else {
                    continue;
                };
                if !sample.has_sdf {
                    continue;
                }

                solid_weight += weight
                    * terrain_solid_occupancy_from_sdf(sample.sdf, dx, occupancy_transition_cells);
            }
        }
    }

    solid_weight.clamp(0.0, 1.0)
}

pub(super) fn terrain_solid_occupancy_from_sdf(
    sdf: f32,
    dx: f32,
    occupancy_transition_cells: f32,
) -> f32 {
    if !sdf.is_finite() || dx <= 0.0 || !dx.is_finite() {
        return 0.0;
    }

    let transition_width = dx * occupancy_transition_cells.max(1.0e-3);
    (0.5 - sdf / transition_width).clamp(0.0, 1.0)
}

fn fluid_density_from_eos_pressure(
    stiffness: f32,
    gamma: f32,
    pressure: f32,
    rest_density: f32,
) -> Option<f32> {
    if !stiffness.is_finite()
        || stiffness <= 0.0
        || !gamma.is_finite()
        || gamma <= 0.0
        || !pressure.is_finite()
        || !rest_density.is_finite()
        || rest_density <= 0.0
    {
        return None;
    }

    let density_ratio = (1.0 + pressure.max(0.0) / stiffness).powf(gamma.recip());
    let density = rest_density * density_ratio;
    (density.is_finite() && density > MIN_FLUID_DENSITY).then_some(density)
}

pub(super) fn fluid_eos_pressure(
    stiffness: f32,
    gamma: f32,
    density: f32,
    rest_density: f32,
    pressure_floor: f32,
) -> f32 {
    if !stiffness.is_finite()
        || stiffness <= 0.0
        || !gamma.is_finite()
        || gamma <= 0.0
        || !density.is_finite()
        || density <= 0.0
        || !rest_density.is_finite()
        || rest_density <= 0.0
        || !pressure_floor.is_finite()
    {
        return 0.0;
    }

    let density_ratio = density / rest_density;
    let pressure = if (gamma - 4.0).abs() <= f32::EPSILON {
        let ratio2 = density_ratio * density_ratio;
        stiffness * (ratio2 * ratio2 - 1.0)
    } else {
        stiffness * (density_ratio.powf(gamma) - 1.0)
    };
    pressure.max(pressure_floor)
}

pub(super) fn fluid_stress(pressure: f32, dynamic_viscosity: f32, velocity_gradient: Mat3) -> Mat3 {
    let pressure = if pressure.is_finite() { pressure } else { 0.0 };
    let dynamic_viscosity = if dynamic_viscosity.is_finite() {
        dynamic_viscosity.max(0.0)
    } else {
        0.0
    };
    let strain_rate = velocity_gradient + velocity_gradient.transpose();
    Mat3::from_diagonal(Vec3::splat(-pressure)) + strain_rate * dynamic_viscosity
}

#[cfg(test)]
pub(super) fn eos_pressure(stiffness: f32, gamma: f32, j: f32, j_min: f32) -> f32 {
    if !stiffness.is_finite()
        || stiffness <= 0.0
        || !gamma.is_finite()
        || gamma <= 0.0
        || !j.is_finite()
    {
        return 0.0;
    }

    // Free-surface weakly-compressible water should resist compression but
    // not generate tensile attraction when a marker's volume estimate is
    // expanded. Negative EOS pressure pulls sparse surface particles into
    // clumps and then lets the pile creep down to j_min; clamp it to zero
    // like a Tait water EOS with no tensile strength.
    let clamped_j = j.max(j_min.max(1.0e-6));
    if clamped_j >= 1.0 {
        return 0.0;
    }

    let compression = if gamma == 7.0 {
        let inv_j = clamped_j.recip();
        let inv_j2 = inv_j * inv_j;
        let inv_j4 = inv_j2 * inv_j2;
        inv_j4 * inv_j2 * inv_j - 1.0
    } else {
        clamped_j.powf(-gamma) - 1.0
    };
    (stiffness * compression).max(0.0)
}

pub(super) fn grid_density_no_tension_j(
    gathered_mass: f32,
    inv_cell_volume: f32,
    rest_density: f32,
    j_min: f32,
) -> Option<f32> {
    if !gathered_mass.is_finite()
        || gathered_mass <= 0.0
        || !inv_cell_volume.is_finite()
        || inv_cell_volume <= 0.0
        || !rest_density.is_finite()
        || rest_density <= 0.0
    {
        return None;
    }

    let density = gathered_mass * inv_cell_volume;
    if !density.is_finite() || density <= 0.0 {
        return None;
    }

    let density_j = rest_density / density;
    if density_j >= NO_TENSION_MAX_J - DENSITY_J_FEEDBACK_DEADBAND.max(0.0) {
        return Some(NO_TENSION_MAX_J);
    }

    Some(clamp_no_tension_j(density_j, j_min))
}

#[cfg(test)]
pub(super) fn density_j_feedback_blend(dt: f32) -> f32 {
    if !dt.is_finite() || dt <= 0.0 || DENSITY_J_FEEDBACK_PER_SECOND <= 0.0 {
        return 0.0;
    }

    (1.0 - (-DENSITY_J_FEEDBACK_PER_SECOND * dt).exp()).clamp(0.0, 1.0)
}

#[cfg(test)]
pub(super) fn blend_no_tension_j(kinematic_j: f32, density_j: f32, blend: f32, j_min: f32) -> f32 {
    let kinematic_j = clamp_no_tension_j(kinematic_j, j_min);
    let density_j = clamp_no_tension_j(density_j, j_min);
    let blend = blend.clamp(0.0, 1.0);
    clamp_no_tension_j(lerp(kinematic_j, density_j, blend), j_min)
}

#[cfg(test)]
pub(super) fn integrate_no_tension_j(j: f32, trace_c: f32, dt: f32, j_min: f32) -> f32 {
    let j = clamp_no_tension_j(j, j_min);
    if !trace_c.is_finite() || !dt.is_finite() || dt <= 0.0 {
        return j;
    }

    // J is a volume-ratio history variable, while the current EOS has no
    // tensile branch for J > 1.  Let compression/relaxation update J
    // multiplicatively in log-space, cap the per-substep change so a clamped
    // APIC affine cannot launch J to an extreme value in one frame, and keep
    // expanded free-surface markers at the rest volume instead of preserving a
    // permanent no-pressure J > 1 history.
    let log_step = (dt * trace_c).clamp(-MAX_J_LOG_STEP_PER_SUBSTEP, MAX_J_LOG_STEP_PER_SUBSTEP);
    clamp_no_tension_j(j * log_step.exp(), j_min)
}

pub(super) fn clamp_no_tension_j(j: f32, j_min: f32) -> f32 {
    let min_j = j_min.clamp(1.0e-6, NO_TENSION_MAX_J);
    if !j.is_finite() {
        return NO_TENSION_MAX_J;
    }

    j.clamp(min_j, NO_TENSION_MAX_J)
}
