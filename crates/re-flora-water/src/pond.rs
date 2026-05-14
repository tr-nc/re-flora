use glam::{Mat3, UVec3, Vec3};

use super::collider::{WaterBoxCollider, WaterTerrainCollider};

const DEFAULT_GRID_DIM: UVec3 = UVec3::new(32, 32, 32);
const DEFAULT_PARTICLE_COUNT: usize = 4_096;

#[derive(Clone, Debug)]
pub struct PondWaterConfig {
    pub collider: WaterBoxCollider,
    pub grid_dim: UVec3,
    pub particle_count: usize,
    pub substep_dt: f32,
    pub particle_mass: f32,
    pub particle_volume: f32,
    pub gravity: Vec3,
    pub stiffness: f32,
    pub gamma: f32,
    pub j_min: f32,
    pub wall_padding_cells: f32,
    pub wall_damping: f32,
}

impl Default for PondWaterConfig {
    fn default() -> Self {
        Self {
            collider: WaterBoxCollider::default(),
            grid_dim: DEFAULT_GRID_DIM,
            particle_count: DEFAULT_PARTICLE_COUNT,
            substep_dt: 1.0 / 240.0,
            particle_mass: 1.0,
            particle_volume: 1.0 / DEFAULT_PARTICLE_COUNT as f32,
            gravity: Vec3::new(0.0, -9.8, 0.0),
            stiffness: 10_000.0,
            gamma: 7.0,
            j_min: 0.1,
            wall_padding_cells: 2.0,
            wall_damping: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WaterParticle {
    pub x: Vec3,
    pub v: Vec3,
    pub c: Mat3,
    pub j: f32,
}

impl WaterParticle {
    fn new(x: Vec3) -> Self {
        Self {
            x,
            v: Vec3::ZERO,
            c: Mat3::ZERO,
            j: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WaterGridNode {
    pub v: Vec3,
    pub mass: f32,
    pub solid: bool,
    pub normal: Vec3,
}

impl Default for WaterGridNode {
    fn default() -> Self {
        Self {
            v: Vec3::ZERO,
            mass: 0.0,
            solid: false,
            normal: Vec3::ZERO,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WaterPerfStats {
    pub substeps: u32,
    pub p2g_seconds: f64,
    pub grid_seconds: f64,
    pub g2p_seconds: f64,
    pub total_seconds: f64,
    pub active_node_visits: u64,
}

impl WaterPerfStats {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

pub struct PondWaterSim {
    pub config: PondWaterConfig,
    pub(crate) terrain: Option<WaterTerrainCollider>,
    pub origin_ws: Vec3,
    pub extent_ws: Vec3,
    pub grid_dim: UVec3,
    pub dx: f32,
    pub inv_dx: f32,
    pub particles: Vec<WaterParticle>,
    pub grid: Vec<WaterGridNode>,
    pub accumulator: f32,
    pub perf_stats: WaterPerfStats,
    pub perf_report_seconds: f32,
}

impl PondWaterSim {
    pub fn new(config: PondWaterConfig) -> Self {
        assert!(config.grid_dim.x >= 4 && config.grid_dim.y >= 4 && config.grid_dim.z >= 4);
        assert!(config.particle_count > 0);

        let origin_ws = config.collider.min_ws;
        let extent_ws = config.collider.extent();
        assert!(extent_ws.min_element() > 0.0);

        let dx = (extent_ws.x / config.grid_dim.x as f32)
            .min(extent_ws.y / config.grid_dim.y as f32)
            .min(extent_ws.z / config.grid_dim.z as f32);
        let inv_dx = dx.recip();

        let grid_len = (config.grid_dim.x as usize)
            .saturating_mul(config.grid_dim.y as usize)
            .saturating_mul(config.grid_dim.z as usize);
        let mut sim = Self {
            config,
            terrain: None,
            origin_ws,
            extent_ws,
            grid_dim: DEFAULT_GRID_DIM,
            dx,
            inv_dx,
            particles: Vec::new(),
            grid: vec![WaterGridNode::default(); grid_len],
            accumulator: 0.0,
            perf_stats: WaterPerfStats::default(),
            perf_report_seconds: 0.0,
        };
        sim.grid_dim = sim.config.grid_dim;
        sim.seed_particles();
        sim
    }

    pub fn fixed_test_box() -> Self {
        Self::new(PondWaterConfig::default())
    }

    pub fn set_terrain_collider(&mut self, collider: WaterTerrainCollider) {
        collider.validate();
        self.terrain = Some(collider);
    }

    pub fn clear_terrain_collider(&mut self) {
        self.terrain = None;
    }

    pub fn terrain_collider(&self) -> Option<&WaterTerrainCollider> {
        self.terrain.as_ref()
    }

    fn seed_particles(&mut self) {
        self.particles.clear();
        self.particles.reserve(self.config.particle_count);

        let side = (self.config.particle_count as f32).cbrt().ceil() as u32;
        let spacing = self.extent_ws / Vec3::splat(side as f32);
        let min = self.origin_ws + self.extent_ws * Vec3::new(0.15, 0.08, 0.15);
        let max = self.origin_ws + self.extent_ws * Vec3::new(0.85, 0.58, 0.85);
        let fill_extent = max - min;

        'z: for z in 0..side {
            for y in 0..side {
                for x in 0..side {
                    if self.particles.len() >= self.config.particle_count {
                        break 'z;
                    }

                    let t = (Vec3::new(x as f32, y as f32, z as f32) + Vec3::splat(0.5))
                        / Vec3::splat(side as f32);
                    let jitter = Vec3::new(
                        hash_unit(x, y, z) - 0.5,
                        hash_unit(y, z, x) - 0.5,
                        hash_unit(z, x, y) - 0.5,
                    ) * spacing
                        * 0.15;
                    let pos = min + fill_extent * t + jitter;
                    self.particles.push(WaterParticle::new(
                        self.config.collider.clamp_point(pos, self.dx),
                    ));
                }
            }
        }
    }
}

fn hash_unit(x: u32, y: u32, z: u32) -> f32 {
    let mut n =
        x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663) ^ z.wrapping_mul(83_492_791);
    n ^= n >> 13;
    n = n.wrapping_mul(1_274_126_177);
    ((n & 0x00ff_ffff) as f32) / 0x0100_0000 as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_test_box_seeds_particles_inside_bounds() {
        let sim = PondWaterSim::fixed_test_box();
        assert_eq!(sim.particles.len(), DEFAULT_PARTICLE_COUNT);
        assert_eq!(sim.grid_dim, DEFAULT_GRID_DIM);
        for particle in &sim.particles {
            assert!(
                sim.config.collider.contains(particle.x),
                "particle escaped: {:?}",
                particle.x
            );
        }
    }

    #[test]
    fn terrain_collider_state_round_trips() {
        let mut sim = PondWaterSim::fixed_test_box();
        assert!(sim.terrain_collider().is_none());

        sim.set_terrain_collider(WaterTerrainCollider {
            xz_dim: glam::UVec2::new(2, 2),
            bounds_min_ws: Vec3::new(1.0, 0.0, 1.0),
            bounds_max_ws: Vec3::new(2.0, 1.0, 2.0),
            heights_ws: vec![1.0, 1.0, 1.0, 1.0],
            margin: 0.0,
        });
        assert!(sim.terrain_collider().is_some());

        sim.clear_terrain_collider();
        assert!(sim.terrain_collider().is_none());
    }
}
