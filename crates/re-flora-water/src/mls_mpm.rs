use super::{
    collider::WaterTerrainColliderSet,
    pond::{WaterGridNode, WaterTerrainGridSample},
};

mod density;
mod diagnostics;
mod g2p;
mod p2g;
mod repair;
mod step;
mod terrain;
mod transfer;

pub use terrain::{
    build_terrain_grid_cache_patch, WaterTerrainCacheApplyReport,
    WaterTerrainCacheBuildRequest, WaterTerrainCachePatch, WaterTerrainCacheRebuildStats,
};
#[cfg(test)]
use density::{
    blend_no_tension_j, density_j_feedback_blend, eos_pressure, fluid_eos_pressure, fluid_stress,
    grid_density_no_tension_j, integrate_no_tension_j, terrain_boundary_density_correction,
    terrain_ghost_density_for_grid_node, terrain_solid_kernel_weight,
    terrain_solid_occupancy_from_sdf,
};
#[cfg(test)]
use diagnostics::{quiet_settling_damping_weight, quiet_settling_local_velocity_weight};
#[cfg(test)]
use repair::{
    affine_damping_factor, collide_particle_with_terrain, collide_particle_with_terrain_iterative,
    damp_velocity_tangent_to_surface, project_velocity_away_from_surface,
    terrain_tangent_damping_factor,
};
#[cfg(test)]
use transfer::{
    grid_node_coord_from_index, terrain_grid_particle_query, TerrainGridParticleQuery,
};

const MAX_SUBSTEPS_PER_UPDATE: usize = 8;
const ACTIVE_MASS_EPSILON: f32 = 1.0e-8;
// A particle can end a substep deeper than one capped correction can resolve.
// Iterate bounded SDF corrections so the next P2G pass does not deposit mass
// from inside terrain.
const TERRAIN_PARTICLE_COLLISION_ITERATIONS: usize = 8;

#[cfg(test)]
mod tests;
