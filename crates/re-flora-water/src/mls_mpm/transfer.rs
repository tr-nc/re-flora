use glam::{IVec3, Vec3};

use super::repair::{
    collide_particle_with_box_with_padding, damp_velocity_tangent_to_surface,
    project_velocity_away_from_surface, terrain_tangent_damping_factor,
};
use crate::pond::{
    WaterGridNode, WaterParticle, WaterTerrainGridSample, WATER_GRID_BOUNDARY_X_MAX,
    WATER_GRID_BOUNDARY_X_MIN, WATER_GRID_BOUNDARY_Y_MAX, WATER_GRID_BOUNDARY_Y_MIN,
    WATER_GRID_BOUNDARY_Z_MAX, WATER_GRID_BOUNDARY_Z_MIN,
};

const TERRAIN_GRID_SKIP_GUARD_CELLS: f32 = 0.25;
const TERRAIN_GRID_PROJECTION_GUARD_CELLS: f32 = 0.10;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TerrainShadowSampleStats {
    pub(super) samples: u64,
    pub(super) false_skips: u64,
    pub(super) sdf_abs_error_sum: f64,
    pub(super) sdf_abs_error_max: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WaterG2pBreakdown {
    pub(super) total_seconds: f64,
    pub(super) gather_seconds: f64,
    pub(super) box_seconds: f64,
    pub(super) terrain_seconds: f64,
    pub(super) repair_seconds: f64,
    pub(super) terrain_cache_skips: u64,
    pub(super) terrain_cache_projections: u64,
    pub(super) terrain_exact_fallbacks: u64,
    pub(super) terrain_exact_checks: u64,
    pub(super) terrain_exact_corrections: u64,
}

pub(super) fn base_coord(grid_pos: Vec3) -> IVec3 {
    let base = (grid_pos - Vec3::splat(0.5)).floor();
    IVec3::new(base.x as i32, base.y as i32, base.z as i32)
}

pub(super) fn quadratic_weights(fx: Vec3) -> [Vec3; 3] {
    let w0 = Vec3::splat(1.5) - fx;
    let w1 = fx - Vec3::ONE;
    let w2 = fx - Vec3::splat(0.5);
    [
        0.5 * w0 * w0,
        Vec3::splat(0.75) - w1 * w1,
        0.5 * w2 * w2,
    ]
}


pub(super) fn in_grid(node: IVec3, grid_dim: glam::UVec3) -> bool {
    node.x >= 0
        && node.y >= 0
        && node.z >= 0
        && node.x < grid_dim.x as i32
        && node.y < grid_dim.y as i32
        && node.z < grid_dim.z as i32
}

pub(super) fn particle_stencil_interior(base: IVec3, grid_dim: glam::UVec3) -> bool {
    base.x >= 0
        && base.y >= 0
        && base.z >= 0
        && base.x < grid_dim.x as i32 - 2
        && base.y < grid_dim.y as i32 - 2
        && base.z < grid_dim.z as i32 - 2
}

pub(super) fn project_grid_node_collisions(
    node: &mut WaterGridNode,
    boundary_flags: u8,
    terrain_sample: WaterTerrainGridSample,
    terrain_collision_margin: f32,
    terrain_tangent_damping_per_sec: f32,
    wall_damping: f32,
    dt: f32,
) {
    let mut normal = Vec3::ZERO;
    // The cached normal band is wider than the actual contact band so G2P can
    // reuse normals near terrain. Grid velocity collision must stay tight;
    // projecting every near-band node makes water hover.
    if terrain_sample.has_sdf
        && terrain_sample.sdf <= terrain_collision_margin
        && terrain_sample.normal.length_squared() > 0.0
    {
        node.v = project_velocity_away_from_surface(node.v, terrain_sample.normal);
        node.v = damp_velocity_tangent_to_surface(
            node.v,
            terrain_sample.normal,
            terrain_tangent_damping_factor(
                terrain_tangent_damping_per_sec,
                terrain_sample.sdf,
                terrain_collision_margin,
                dt,
            ),
        );
        normal += terrain_sample.normal;
    }

    if boundary_flags & WATER_GRID_BOUNDARY_X_MIN != 0 && node.v.x < 0.0 {
        node.v.x *= -wall_damping;
        normal += Vec3::X;
    }
    if boundary_flags & WATER_GRID_BOUNDARY_X_MAX != 0 && node.v.x > 0.0 {
        node.v.x *= -wall_damping;
        normal -= Vec3::X;
    }
    if boundary_flags & WATER_GRID_BOUNDARY_Y_MIN != 0 && node.v.y < 0.0 {
        node.v.y *= -wall_damping;
        normal += Vec3::Y;
    }
    if boundary_flags & WATER_GRID_BOUNDARY_Y_MAX != 0 && node.v.y > 0.0 {
        node.v.y *= -wall_damping;
        normal -= Vec3::Y;
    }
    if boundary_flags & WATER_GRID_BOUNDARY_Z_MIN != 0 && node.v.z < 0.0 {
        node.v.z *= -wall_damping;
        normal += Vec3::Z;
    }
    if boundary_flags & WATER_GRID_BOUNDARY_Z_MAX != 0 && node.v.z > 0.0 {
        node.v.z *= -wall_damping;
        normal -= Vec3::Z;
    }

    if normal.length_squared() > 0.0 {
        node.solid = true;
        node.normal = normal.normalize_or_zero();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum TerrainGridParticleQuery {
    Skip { sdf: f32 },
    CachedProjection {
        sdf: f32,
        normal: Vec3,
        cached_sdf: f32,
    },
    ExactFallback,
}

pub(super) fn terrain_grid_particle_query(
    local_pos: Vec3,
    inv_dx: f32,
    dx: f32,
    grid_dim: glam::UVec3,
    terrain_grid: &[WaterTerrainGridSample],
    collision_margin: f32,
) -> TerrainGridParticleQuery {
    if terrain_grid.is_empty()
        || !local_pos.is_finite()
        || inv_dx <= 0.0
        || !inv_dx.is_finite()
        || dx <= 0.0
        || !dx.is_finite()
    {
        return TerrainGridParticleQuery::ExactFallback;
    }

    let grid_pos = local_pos * inv_dx;
    if !grid_pos.is_finite() {
        return TerrainGridParticleQuery::ExactFallback;
    }

    let base = grid_pos.floor().as_ivec3();
    let next = base + IVec3::ONE;
    if !in_grid(base, grid_dim) || !in_grid(next, grid_dim) {
        return TerrainGridParticleQuery::ExactFallback;
    }

    let f = (grid_pos - base.as_vec3()).clamp(Vec3::ZERO, Vec3::ONE);
    let mut corner_sdf = [[[0.0f32; 2]; 2]; 2];
    for oz in 0..=1 {
        for oy in 0..=1 {
            for ox in 0..=1 {
                let node = base + IVec3::new(ox, oy, oz);
                let idx = grid_index_dims(grid_dim, node.x as u32, node.y as u32, node.z as u32);
                let Some(sample) = terrain_grid.get(idx) else {
                    return TerrainGridParticleQuery::ExactFallback;
                };
                if !sample.has_sdf {
                    return TerrainGridParticleQuery::ExactFallback;
                }
                corner_sdf[oz as usize][oy as usize][ox as usize] = sample.sdf;
            }
        }
    }

    let sdf = trilinear_sdf(corner_sdf, f);
    let collision_margin = collision_margin.max(0.0);
    let skip_guard = dx * TERRAIN_GRID_SKIP_GUARD_CELLS;
    if sdf > collision_margin + skip_guard {
        return TerrainGridParticleQuery::Skip { sdf };
    }

    let normal = trilinear_sdf_gradient(corner_sdf, f).normalize_or_zero();
    if normal.is_finite() && normal.length_squared() > 0.0 {
        let projection_sdf = if sdf <= collision_margin {
            sdf
        } else {
            // In the interpolation-uncertainty band just outside the contact
            // margin, apply a small conservative cached correction instead of
            // falling back to the exact collider for every near-surface particle.
            sdf - dx * TERRAIN_GRID_PROJECTION_GUARD_CELLS
        };
        return TerrainGridParticleQuery::CachedProjection {
            sdf: projection_sdf,
            normal,
            cached_sdf: sdf,
        };
    }

    TerrainGridParticleQuery::ExactFallback
}

pub(super) fn should_shadow_sample_terrain(_particle_idx: usize) -> bool {
    true
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn trilinear_sdf(c: [[[f32; 2]; 2]; 2], f: Vec3) -> f32 {
    let x00 = lerp(c[0][0][0], c[0][0][1], f.x);
    let x10 = lerp(c[0][1][0], c[0][1][1], f.x);
    let x01 = lerp(c[1][0][0], c[1][0][1], f.x);
    let x11 = lerp(c[1][1][0], c[1][1][1], f.x);
    let y0 = lerp(x00, x10, f.y);
    let y1 = lerp(x01, x11, f.y);
    lerp(y0, y1, f.z)
}

fn trilinear_sdf_gradient(c: [[[f32; 2]; 2]; 2], f: Vec3) -> Vec3 {
    let dx00 = c[0][0][1] - c[0][0][0];
    let dx10 = c[0][1][1] - c[0][1][0];
    let dx01 = c[1][0][1] - c[1][0][0];
    let dx11 = c[1][1][1] - c[1][1][0];
    let dy00 = c[0][1][0] - c[0][0][0];
    let dy10 = c[0][1][1] - c[0][0][1];
    let dy01 = c[1][1][0] - c[1][0][0];
    let dy11 = c[1][1][1] - c[1][0][1];
    let dz00 = c[1][0][0] - c[0][0][0];
    let dz10 = c[1][0][1] - c[0][0][1];
    let dz01 = c[1][1][0] - c[0][1][0];
    let dz11 = c[1][1][1] - c[0][1][1];

    let grad_x = lerp(lerp(dx00, dx10, f.y), lerp(dx01, dx11, f.y), f.z);
    let grad_y = lerp(lerp(dy00, dy10, f.x), lerp(dy01, dy11, f.x), f.z);
    let grad_z = lerp(lerp(dz00, dz10, f.x), lerp(dz01, dz11, f.x), f.y);
    Vec3::new(grad_x, grad_y, grad_z)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn project_particle_with_cached_terrain(
    particle: &mut WaterParticle,
    sdf: f32,
    terrain_normal: Vec3,
    collision_margin: f32,
    max_correction: f32,
    box_min_ws: Vec3,
    box_max_ws: Vec3,
    box_min_padding: Vec3,
    box_max_padding: Vec3,
) {
    let correction = collision_margin.max(0.0) - sdf;
    if correction <= 0.0 {
        return;
    }

    particle.x += terrain_normal * correction.min(max_correction.max(0.0));
    particle.v = project_velocity_away_from_surface(particle.v, terrain_normal);
    collide_particle_with_box_with_padding(
        particle,
        box_min_ws,
        box_max_ws,
        box_min_padding,
        box_max_padding,
    );
}

pub(super) fn grid_index_dims(grid_dim: glam::UVec3, x: u32, y: u32, z: u32) -> usize {
    ((z as usize * grid_dim.y as usize + y as usize) * grid_dim.x as usize) + x as usize
}

pub(super) fn grid_node_coord_from_index(grid_dim: glam::UVec3, idx: usize) -> Option<glam::UVec3> {
    let x_dim = grid_dim.x as usize;
    let y_dim = grid_dim.y as usize;
    let z_dim = grid_dim.z as usize;
    if x_dim == 0 || y_dim == 0 || z_dim == 0 {
        return None;
    }
    let layer = x_dim.checked_mul(y_dim)?;
    let node_count = layer.checked_mul(z_dim)?;
    if idx >= node_count {
        return None;
    }

    let z = idx / layer;
    let rem = idx - z * layer;
    let y = rem / x_dim;
    let x = rem - y * x_dim;
    Some(glam::UVec3::new(x as u32, y as u32, z as u32))
}
