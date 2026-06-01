use glam::Vec3;

use super::{density::MAX_J, repair::mat3_is_finite};
use crate::{
    collider::{WaterBoxCollider, WaterTerrainColliderSet},
    pond::WaterParticle,
};

// Quiet puddles can retain low-energy numerical circulation indefinitely. Apply
// extra damping only after the whole water body is already slow; fast
// falling/splashing water bypasses this path so it does not turn the material
// into honey.
const QUIET_SETTLING_AVG_SPEED_THRESHOLD: f32 = 0.08;
const QUIET_SETTLING_MAX_SPEED_THRESHOLD: f32 = 0.40;
const QUIET_SETTLING_LOCAL_SPEED_THRESHOLD: f32 = 0.35;

#[derive(Clone, Copy, Debug)]
pub(super) struct QuietMotionSample {
    pub(super) avg_speed: f32,
    pub(super) max_speed: f32,
}

pub(super) fn quiet_motion_sample(particles: &[WaterParticle]) -> Option<QuietMotionSample> {
    let mut finite_particles = 0usize;
    let mut sum_speed = 0.0f32;
    let mut max_speed = 0.0f32;

    for particle in particles {
        if !particle.v.is_finite() {
            continue;
        }

        finite_particles += 1;
        let speed = particle.v.length();
        sum_speed += speed;
        max_speed = max_speed.max(speed);
    }

    if finite_particles == 0 {
        return None;
    }

    Some(QuietMotionSample {
        avg_speed: sum_speed / finite_particles as f32,
        max_speed,
    })
}

pub(super) fn quiet_settling_damping_weight(
    avg_speed: f32,
    max_speed: f32,
    velocity_damping_per_sec: f32,
    affine_damping_per_sec: f32,
) -> f32 {
    if !avg_speed.is_finite()
        || !max_speed.is_finite()
        || (velocity_damping_per_sec <= 0.0 && affine_damping_per_sec <= 0.0)
    {
        return 0.0;
    }

    let avg_weight = 1.0
        - smoothstep(
            QUIET_SETTLING_AVG_SPEED_THRESHOLD,
            QUIET_SETTLING_AVG_SPEED_THRESHOLD * 2.0,
            avg_speed,
        );
    let max_weight = 1.0
        - smoothstep(
            QUIET_SETTLING_MAX_SPEED_THRESHOLD,
            QUIET_SETTLING_MAX_SPEED_THRESHOLD * 2.0,
            max_speed,
        );
    avg_weight.min(max_weight).clamp(0.0, 1.0)
}

pub(super) fn quiet_settling_local_velocity_weight(speed: f32) -> f32 {
    if !speed.is_finite() {
        return 0.0;
    }

    (1.0
        - smoothstep(
            QUIET_SETTLING_LOCAL_SPEED_THRESHOLD,
            QUIET_SETTLING_LOCAL_SPEED_THRESHOLD * 2.0,
            speed,
        ))
    .clamp(0.0, 1.0)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if !edge0.is_finite() || !edge1.is_finite() || edge1 <= edge0 {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }

    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct WaterParticleDebugStats {
    pub(super) finite_particles: usize,
    pub(super) min_ws: Vec3,
    pub(super) max_ws: Vec3,
    pub(super) avg_ws: Vec3,
    pub(super) avg_speed: f32,
    pub(super) max_speed: f32,
    pub(super) speed_limited_particles: usize,
    pub(super) max_speed_index: usize,
    pub(super) max_speed_position: Vec3,
    pub(super) max_speed_velocity: Vec3,
    pub(super) max_speed_j: f32,
    pub(super) max_speed_terrain_sdf: Option<f32>,
    pub(super) min_j: f32,
    pub(super) max_j: f32,
    pub(super) j_min_clamped_particles: usize,
    pub(super) j_max_clamped_particles: usize,
    pub(super) max_abs_affine: f32,
    pub(super) min_terrain_sdf: Option<f32>,
    pub(super) max_terrain_penetration: f32,
    pub(super) terrain_contact_particles: usize,
    pub(super) terrain_penetrating: usize,
    pub(super) no_terrain_sdf: usize,
    pub(super) floor_pinned_particles: usize,
    pub(super) ceiling_pinned_particles: usize,
    pub(super) wall_pinned_particles: usize,
    pub(super) out_of_bounds_particles: usize,
    pub(super) non_finite_particles: usize,
}

pub(super) fn water_particle_debug_stats(
    particles: &[WaterParticle],
    terrain: Option<&WaterTerrainColliderSet>,
    bounds: WaterBoxCollider,
    padding: f32,
    speed_limit: f32,
    eos_j_min: Option<f32>,
    terrain_collision_margin: f32,
) -> WaterParticleDebugStats {
    if particles.is_empty() {
        return WaterParticleDebugStats {
            finite_particles: 0,
            min_ws: Vec3::splat(f32::NAN),
            max_ws: Vec3::splat(f32::NAN),
            avg_ws: Vec3::splat(f32::NAN),
            avg_speed: f32::NAN,
            max_speed: f32::NAN,
            speed_limited_particles: 0,
            max_speed_index: 0,
            max_speed_position: Vec3::splat(f32::NAN),
            max_speed_velocity: Vec3::splat(f32::NAN),
            max_speed_j: f32::NAN,
            max_speed_terrain_sdf: None,
            min_j: f32::NAN,
            max_j: f32::NAN,
            j_min_clamped_particles: 0,
            j_max_clamped_particles: 0,
            max_abs_affine: f32::NAN,
            min_terrain_sdf: None,
            max_terrain_penetration: 0.0,
            terrain_contact_particles: 0,
            terrain_penetrating: 0,
            no_terrain_sdf: 0,
            floor_pinned_particles: 0,
            ceiling_pinned_particles: 0,
            wall_pinned_particles: 0,
            out_of_bounds_particles: 0,
            non_finite_particles: 0,
        };
    }

    let padded_min = bounds.min_ws + Vec3::splat(padding);
    let padded_max = bounds.max_ws - Vec3::splat(padding);
    let boundary_epsilon = (padding * 0.1).max(1.0e-4);
    let speed_limit_threshold = speed_limit * 0.98;
    let track_j_min = eos_j_min.is_some();
    let j_min_threshold = eos_j_min.unwrap_or(1.0) * 1.001;
    let j_max_threshold = MAX_J * 0.999;

    let mut finite_particles = 0usize;
    let mut non_finite_particles = 0usize;
    let mut min_ws = Vec3::splat(f32::INFINITY);
    let mut max_ws = Vec3::splat(f32::NEG_INFINITY);
    let mut sum_ws = Vec3::ZERO;
    let mut sum_speed = 0.0f32;
    let mut max_speed = 0.0f32;
    let mut speed_limited_particles = 0usize;
    let mut max_speed_index = 0usize;
    let mut max_speed_position = Vec3::ZERO;
    let mut max_speed_velocity = Vec3::ZERO;
    let mut max_speed_j = 1.0f32;
    let mut max_speed_terrain_sdf = None;
    let mut min_j = if track_j_min { f32::INFINITY } else { 1.0 };
    let mut max_j = if track_j_min { f32::NEG_INFINITY } else { 1.0 };
    let mut j_min_clamped_particles = 0usize;
    let mut j_max_clamped_particles = 0usize;
    let mut max_abs_affine = 0.0f32;
    let mut min_terrain_sdf = f32::INFINITY;
    let mut max_terrain_penetration = 0.0f32;
    let mut terrain_contact_particles = 0usize;
    let mut terrain_penetrating = 0usize;
    let mut no_terrain_sdf = 0usize;
    let mut floor_pinned_particles = 0usize;
    let mut ceiling_pinned_particles = 0usize;
    let mut wall_pinned_particles = 0usize;
    let mut out_of_bounds_particles = 0usize;

    for (particle_idx, particle) in particles.iter().enumerate() {
        if !particle.x.is_finite()
            || !particle.v.is_finite()
            || (track_j_min && !particle.j.is_finite())
            || !mat3_is_finite(particle.c)
        {
            non_finite_particles += 1;
            continue;
        }

        finite_particles += 1;
        min_ws = min_ws.min(particle.x);
        max_ws = max_ws.max(particle.x);
        sum_ws += particle.x;

        if !bounds.contains(particle.x) {
            out_of_bounds_particles += 1;
        }
        if particle.x.y <= padded_min.y + boundary_epsilon {
            floor_pinned_particles += 1;
        }
        if particle.x.y >= padded_max.y - boundary_epsilon {
            ceiling_pinned_particles += 1;
        }
        if particle.x.x <= padded_min.x + boundary_epsilon
            || particle.x.x >= padded_max.x - boundary_epsilon
            || particle.x.z <= padded_min.z + boundary_epsilon
            || particle.x.z >= padded_max.z - boundary_epsilon
        {
            wall_pinned_particles += 1;
        }

        let speed = particle.v.length();
        sum_speed += speed;
        if speed >= speed_limit_threshold {
            speed_limited_particles += 1;
        }

        let terrain_sdf = terrain.and_then(|terrain| terrain.sample_sdf_ws(particle.x));
        if let Some(sdf) = terrain_sdf {
            min_terrain_sdf = min_terrain_sdf.min(sdf);
            if sdf <= terrain_collision_margin {
                terrain_contact_particles += 1;
            }
            if sdf < 0.0 {
                terrain_penetrating += 1;
                max_terrain_penetration = max_terrain_penetration.max(-sdf);
            }
        } else if terrain.is_some() {
            no_terrain_sdf += 1;
        }

        if speed > max_speed {
            max_speed = speed;
            max_speed_index = particle_idx;
            max_speed_position = particle.x;
            max_speed_velocity = particle.v;
            max_speed_j = if track_j_min { particle.j } else { 1.0 };
            max_speed_terrain_sdf = terrain_sdf;
        }

        if track_j_min {
            min_j = min_j.min(particle.j);
            max_j = max_j.max(particle.j);
            if particle.j <= j_min_threshold {
                j_min_clamped_particles += 1;
            }
            if particle.j >= j_max_threshold {
                j_max_clamped_particles += 1;
            }
        }
        max_abs_affine = max_abs_affine
            .max(particle.c.x_axis.abs().max_element())
            .max(particle.c.y_axis.abs().max_element())
            .max(particle.c.z_axis.abs().max_element());
    }

    if finite_particles == 0 {
        return WaterParticleDebugStats {
            finite_particles,
            min_ws: Vec3::splat(f32::NAN),
            max_ws: Vec3::splat(f32::NAN),
            avg_ws: Vec3::splat(f32::NAN),
            avg_speed: f32::NAN,
            max_speed: f32::NAN,
            speed_limited_particles,
            max_speed_index,
            max_speed_position: Vec3::splat(f32::NAN),
            max_speed_velocity: Vec3::splat(f32::NAN),
            max_speed_j: f32::NAN,
            max_speed_terrain_sdf: None,
            min_j: f32::NAN,
            max_j: f32::NAN,
            j_min_clamped_particles,
            j_max_clamped_particles,
            max_abs_affine: f32::NAN,
            min_terrain_sdf: None,
            max_terrain_penetration,
            terrain_contact_particles,
            terrain_penetrating,
            no_terrain_sdf,
            floor_pinned_particles,
            ceiling_pinned_particles,
            wall_pinned_particles,
            out_of_bounds_particles,
            non_finite_particles,
        };
    }

    WaterParticleDebugStats {
        finite_particles,
        min_ws,
        max_ws,
        avg_ws: sum_ws / finite_particles as f32,
        avg_speed: sum_speed / finite_particles as f32,
        max_speed,
        speed_limited_particles,
        max_speed_index,
        max_speed_position,
        max_speed_velocity,
        max_speed_j,
        max_speed_terrain_sdf,
        min_j,
        max_j,
        j_min_clamped_particles,
        j_max_clamped_particles,
        max_abs_affine,
        min_terrain_sdf: min_terrain_sdf.is_finite().then_some(min_terrain_sdf),
        max_terrain_penetration,
        terrain_contact_particles,
        terrain_penetrating,
        no_terrain_sdf,
        floor_pinned_particles,
        ceiling_pinned_particles,
        wall_pinned_particles,
        out_of_bounds_particles,
        non_finite_particles,
    }
}
