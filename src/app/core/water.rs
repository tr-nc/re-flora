use super::App;
use glam::{UVec2, Vec2};
use re_flora_water::WaterTerrainCollider;

const WATER_TERRAIN_XZ_DIM: UVec2 = UVec2::new(32, 32);

impl App {
    pub(super) fn refresh_water_terrain_collider(&mut self) {
        let bounds = self.water_sim.config.collider;
        let bounds_min_ws = bounds.min_ws;
        let bounds_max_ws = bounds.max_ws;
        let sample_count = (WATER_TERRAIN_XZ_DIM.x as usize) * (WATER_TERRAIN_XZ_DIM.y as usize);
        let mut heights_ws = Vec::with_capacity(sample_count);
        let mut min_height = f32::INFINITY;
        let mut max_height = f32::NEG_INFINITY;
        let mut sum_height = 0.0f32;

        for z in 0..WATER_TERRAIN_XZ_DIM.y {
            let tz = z as f32 / (WATER_TERRAIN_XZ_DIM.y - 1) as f32;
            let world_z = bounds_min_ws.z + (bounds_max_ws.z - bounds_min_ws.z) * tz;
            for x in 0..WATER_TERRAIN_XZ_DIM.x {
                let tx = x as f32 / (WATER_TERRAIN_XZ_DIM.x - 1) as f32;
                let world_x = bounds_min_ws.x + (bounds_max_ws.x - bounds_min_ws.x) * tx;
                let height = self.query_terrain_height_cpu(Vec2::new(world_x, world_z));
                min_height = min_height.min(height);
                max_height = max_height.max(height);
                sum_height += height;
                heights_ws.push(height);
            }
        }

        let avg_height = sum_height / heights_ws.len().max(1) as f32;
        let margin = self.water_sim.dx * 0.5;
        self.water_sim.set_terrain_collider(WaterTerrainCollider {
            xz_dim: WATER_TERRAIN_XZ_DIM,
            bounds_min_ws,
            bounds_max_ws,
            heights_ws,
            margin,
        });

        log::info!(
            "[WATER][TERRAIN] sampled {}x{} heights min {:.3} max {:.3} avg {:.3} pond_y {:.3}..{:.3}",
            WATER_TERRAIN_XZ_DIM.x,
            WATER_TERRAIN_XZ_DIM.y,
            min_height,
            max_height,
            avg_height,
            bounds_min_ws.y,
            bounds_max_ws.y,
        );
    }
}
