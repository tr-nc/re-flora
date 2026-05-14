use super::App;
use glam::{UVec2, Vec2, Vec3};
use re_flora_water::WaterTerrainCollider;

const WATER_TERRAIN_XZ_DIM: UVec2 = UVec2::new(32, 32);

impl App {
    pub(super) fn water_terrain_sphere_overlaps(&self, center_ws: Vec3, radius_ws: f32) -> bool {
        let radius_ws = radius_ws.max(0.0);
        self.water_terrain_world_xz_aabb_overlaps(
            Vec2::new(center_ws.x - radius_ws, center_ws.z - radius_ws),
            Vec2::new(center_ws.x + radius_ws, center_ws.z + radius_ws),
        )
    }

    pub(super) fn invalidate_water_terrain_collider_for_overlapping_edit(&mut self) {
        if self.water_terrain_initialized {
            log::debug!("[WATER][TERRAIN] invalidated by overlapping terrain edit");
        }
        self.water_terrain_initialized = false;
    }

    pub(super) fn water_terrain_refresh_ready(&mut self) -> bool {
        if !self.deferred_chunk_rebuilds_idle() {
            log::debug!("[WATER][TERRAIN] refresh deferred until terrain rebuild queue is idle");
            return false;
        }

        self.contree_builder
            .poll_cpu_chunk_cache_jobs(self.tracer.camera_position(), super::VOXEL_DIM_PER_CHUNK);
        let ready = self.contree_builder.cpu_chunk_cache_jobs_idle();
        if !ready {
            log::debug!("[WATER][TERRAIN] refresh deferred until CPU terrain cache jobs finish");
        }
        ready
    }

    fn water_terrain_world_xz_aabb_overlaps(&self, min_xz: Vec2, max_xz: Vec2) -> bool {
        let pond = self.water_sim.config.collider;
        xz_aabb_overlaps(
            min_xz,
            max_xz,
            Vec2::new(pond.min_ws.x, pond.min_ws.z),
            Vec2::new(pond.max_ws.x, pond.max_ws.z),
        )
    }

    pub(super) fn refresh_water_terrain_collider(&mut self) {
        let bounds = self.water_sim.config.collider;
        let bounds_min_ws = bounds.min_ws;
        let bounds_max_ws = bounds.max_ws;
        let sample_count = (WATER_TERRAIN_XZ_DIM.x as usize) * (WATER_TERRAIN_XZ_DIM.y as usize);
        let mut heights_ws = Vec::with_capacity(sample_count);
        let mut min_height = f32::INFINITY;
        let mut max_height = f32::NEG_INFINITY;
        let mut sum_height = 0.0f32;
        let mut above_pond_samples = 0usize;
        let mut below_pond_samples = 0usize;

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
                if height > bounds_max_ws.y {
                    above_pond_samples += 1;
                }
                if height < bounds_min_ws.y {
                    below_pond_samples += 1;
                }
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
        if above_pond_samples > 0 || below_pond_samples > 0 {
            log::warn!(
                "[WATER][TERRAIN] {} / {} samples above pond and {} / {} below pond; collision response is clamped to the pond box interior",
                above_pond_samples,
                sample_count,
                below_pond_samples,
                sample_count,
            );
        }
    }
}

fn xz_aabb_overlaps(min_a: Vec2, max_a: Vec2, min_b: Vec2, max_b: Vec2) -> bool {
    min_a.x <= max_b.x && max_a.x >= min_b.x && min_a.y <= max_b.y && max_a.y >= min_b.y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xz_overlap_includes_shared_edges() {
        assert!(xz_aabb_overlaps(
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.5),
            Vec2::new(2.0, 1.5),
        ));
    }

    #[test]
    fn xz_overlap_rejects_separated_bounds() {
        assert!(!xz_aabb_overlaps(
            Vec2::new(0.0, 0.0),
            Vec2::new(0.9, 1.0),
            Vec2::new(1.0, 0.5),
            Vec2::new(2.0, 1.5),
        ));
    }
}
