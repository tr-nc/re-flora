use glam::{Mat3, Vec3};

use super::{density::clamp_no_tension_j, density::NO_TENSION_MAX_J, WaterTerrainColliderSet};
use crate::pond::WaterParticle;

const APIC_AFFINE_DAMPING_PER_SECOND: f32 = 1.5;
const MAX_PARTICLE_SPEED: f32 = 20.0;
const MAX_PARTICLE_CFL_CELLS_PER_SUBSTEP: f32 = 0.5;
pub(super) const MAX_AFFINE_COMPONENT: f32 = 100.0;

pub(super) fn affine_damping_factor(dt: f32) -> f32 {
    if !dt.is_finite() || dt <= 0.0 || APIC_AFFINE_DAMPING_PER_SECOND <= 0.0 {
        return 1.0;
    }

    (-APIC_AFFINE_DAMPING_PER_SECOND * dt)
        .exp()
        .clamp(0.0, 1.0)
}

pub(super) fn damp_velocity_tangent_to_surface(
    velocity: Vec3,
    normal: Vec3,
    tangent_damping: f32,
) -> Vec3 {
    if !velocity.is_finite() {
        return Vec3::ZERO;
    }
    if !normal.is_finite() {
        return velocity;
    }

    let normal = normal.normalize_or_zero();
    if normal.length_squared() <= f32::EPSILON {
        return velocity;
    }

    let tangent_damping = tangent_damping.clamp(0.0, 1.0);
    let normal_speed = velocity.dot(normal);
    let normal_v = normal * normal_speed;
    let tangent_v = velocity - normal_v;
    normal_v + tangent_v * tangent_damping
}

pub(super) fn terrain_tangent_damping_factor(
    damping_per_sec: f32,
    terrain_sdf: f32,
    collision_margin: f32,
    dt: f32,
) -> f32 {
    if damping_per_sec <= 0.0
        || !damping_per_sec.is_finite()
        || !terrain_sdf.is_finite()
        || dt <= 0.0
        || !dt.is_finite()
    {
        return 1.0;
    }

    let collision_margin = collision_margin.max(0.0);
    let contact_weight = if collision_margin > f32::EPSILON {
        ((collision_margin - terrain_sdf) / collision_margin).clamp(0.0, 1.0)
    } else if terrain_sdf <= 0.0 {
        1.0
    } else {
        0.0
    };

    if contact_weight <= 0.0 {
        return 1.0;
    }

    (-damping_per_sec * contact_weight * dt)
        .exp()
        .clamp(0.0, 1.0)
}

pub(super) fn project_velocity_away_from_surface(velocity: Vec3, normal: Vec3) -> Vec3 {
    if !velocity.is_finite() {
        return Vec3::ZERO;
    }
    if !normal.is_finite() {
        return velocity;
    }

    let normal = normal.normalize_or_zero();
    if normal.length_squared() <= f32::EPSILON {
        return velocity;
    }

    let inward_speed = velocity.dot(normal);
    if inward_speed < 0.0 {
        velocity - normal * inward_speed
    } else {
        velocity
    }
}

pub(super) fn repair_particle_state_with_padding(
    particle: &mut WaterParticle,
    min_ws: Vec3,
    max_ws: Vec3,
    min_padding: Vec3,
    max_padding: Vec3,
    max_speed: f32,
    j_min: f32,
) {
    repair_particle_position_velocity_with_padding(
        particle,
        min_ws,
        max_ws,
        min_padding,
        max_padding,
        max_speed,
    );

    if !mat3_is_finite(particle.c) {
        particle.c = Mat3::ZERO;
    }
    particle.c = clamp_mat3_components(particle.c, MAX_AFFINE_COMPONENT);
    particle.j = clamp_no_tension_j(particle.j, j_min);
}

pub(super) fn repair_particle_state_after_g2p_with_padding(
    particle: &mut WaterParticle,
    min_ws: Vec3,
    max_ws: Vec3,
    min_padding: Vec3,
    max_padding: Vec3,
    max_speed: f32,
    j_min: f32,
) {
    let min = min_ws + min_padding;
    let max = max_ws - max_padding;
    if particle.x.is_finite()
        && particle.v.is_finite()
        && mat3_is_finite(particle.c)
        && particle.j.is_finite()
        && particle.j >= j_min
        && particle.j <= NO_TENSION_MAX_J
        && !particle.x.cmplt(min).any()
        && !particle.x.cmpgt(max).any()
    {
        return;
    }

    repair_particle_state_with_padding(
        particle,
        min_ws,
        max_ws,
        min_padding,
        max_padding,
        max_speed,
        j_min,
    );
}

pub(super) fn repair_particle_position_velocity_with_padding(
    particle: &mut WaterParticle,
    min_ws: Vec3,
    max_ws: Vec3,
    min_padding: Vec3,
    max_padding: Vec3,
    max_speed: f32,
) {
    let min = min_ws + min_padding;
    let max = max_ws - max_padding;
    let fallback = (min + max) * 0.5;
    particle.x = finite_or(particle.x, fallback).clamp(min, max);

    if !particle.v.is_finite() {
        particle.v = Vec3::ZERO;
    }
    particle.v = clamp_vec3_length(particle.v, max_speed);
}

pub(super) fn max_particle_speed_for_substep(dx: f32, dt: f32) -> f32 {
    if dx > 0.0 && dt > 0.0 && dx.is_finite() && dt.is_finite() {
        MAX_PARTICLE_SPEED.min(MAX_PARTICLE_CFL_CELLS_PER_SUBSTEP * dx / dt)
    } else {
        MAX_PARTICLE_SPEED
    }
}

pub(super) fn velocity_damping_factor(linear_damping_per_sec: f32, dt: f32) -> f32 {
    if linear_damping_per_sec <= 0.0
        || !linear_damping_per_sec.is_finite()
        || dt <= 0.0
        || !dt.is_finite()
    {
        return 1.0;
    }

    (-linear_damping_per_sec * dt).exp().clamp(0.0, 1.0)
}

fn finite_or(value: Vec3, fallback: Vec3) -> Vec3 {
    Vec3::new(
        if value.x.is_finite() {
            value.x
        } else {
            fallback.x
        },
        if value.y.is_finite() {
            value.y
        } else {
            fallback.y
        },
        if value.z.is_finite() {
            value.z
        } else {
            fallback.z
        },
    )
}

pub(super) fn clamp_vec3_length(value: Vec3, max_length: f32) -> Vec3 {
    let max_length = max_length.max(0.0);
    let length_squared = value.length_squared();
    if length_squared > max_length * max_length {
        value * (max_length / length_squared.sqrt())
    } else {
        value
    }
}

pub(super) fn clamp_mat3_components(value: Mat3, max_abs_component: f32) -> Mat3 {
    let limit = Vec3::splat(max_abs_component.max(0.0));
    Mat3::from_cols(
        value.x_axis.clamp(-limit, limit),
        value.y_axis.clamp(-limit, limit),
        value.z_axis.clamp(-limit, limit),
    )
}

pub(super) fn mat3_is_finite(value: Mat3) -> bool {
    value.x_axis.is_finite() && value.y_axis.is_finite() && value.z_axis.is_finite()
}

pub(super) fn collide_particle_with_box_with_padding(
    particle: &mut WaterParticle,
    min_ws: Vec3,
    max_ws: Vec3,
    min_padding: Vec3,
    max_padding: Vec3,
) {
    let min = min_ws + min_padding;
    let max = max_ws - max_padding;

    if particle.x.x < min.x {
        particle.x.x = min.x;
        particle.v.x = particle.v.x.max(0.0);
    } else if particle.x.x > max.x {
        particle.x.x = max.x;
        particle.v.x = particle.v.x.min(0.0);
    }

    if particle.x.y < min.y {
        particle.x.y = min.y;
        particle.v.y = particle.v.y.max(0.0);
    } else if particle.x.y > max.y {
        particle.x.y = max.y;
        particle.v.y = particle.v.y.min(0.0);
    }

    if particle.x.z < min.z {
        particle.x.z = min.z;
        particle.v.z = particle.v.z.max(0.0);
    } else if particle.x.z > max.z {
        particle.x.z = max.z;
        particle.v.z = particle.v.z.min(0.0);
    }
}

pub(super) fn collide_particle_with_terrain(
    particle: &mut WaterParticle,
    terrain: &WaterTerrainColliderSet,
    collision_margin: f32,
    max_correction: f32,
) -> bool {
    let Some(sdf) = terrain.sample_sdf_ws(particle.x) else {
        return false;
    };

    let correction = collision_margin.max(0.0) - sdf;
    if correction <= 0.0 {
        return false;
    }

    let terrain_normal = terrain.sample_normal_ws(particle.x).unwrap_or(Vec3::Y);
    particle.x += terrain_normal * correction.min(max_correction.max(0.0));
    particle.v = project_velocity_away_from_surface(particle.v, terrain_normal);
    true
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collide_particle_with_terrain_iterative(
    particle: &mut WaterParticle,
    terrain: &WaterTerrainColliderSet,
    collision_margin: f32,
    max_correction: f32,
    iterations: usize,
    box_min_ws: Vec3,
    box_max_ws: Vec3,
    box_min_padding: Vec3,
    box_max_padding: Vec3,
) -> bool {
    let mut corrected = false;
    for _ in 0..iterations {
        let before = particle.x;
        if !collide_particle_with_terrain(particle, terrain, collision_margin, max_correction) {
            return corrected;
        }
        corrected = true;
        collide_particle_with_box_with_padding(
            particle,
            box_min_ws,
            box_max_ws,
            box_min_padding,
            box_max_padding,
        );
        if particle.x.distance_squared(before) <= 1.0e-10 {
            return corrected;
        }
    }
    corrected
}
