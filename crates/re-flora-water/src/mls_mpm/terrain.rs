use glam::{IVec3, UVec3, Vec3};
use std::time::Instant;

use super::grid_index_dims;
use crate::{
    collider::WaterTerrainColliderSet,
    pond::{PondWaterConfig, PondWaterSim, WaterTerrainGridSample},
};

#[derive(Debug)]
pub struct WaterTerrainCacheBuildRequest {
    chunk_id: IVec3,
    terrain_chunk_count: usize,
    terrain: Option<WaterTerrainColliderSet>,
    origin_ws: Vec3,
    grid_dim: UVec3,
    dx: f32,
    min_node: UVec3,
    max_node_exclusive: UVec3,
    near_surface_band: f32,
}

impl WaterTerrainCacheBuildRequest {
    pub fn for_config_and_terrain(
        config: &PondWaterConfig,
        terrain: Option<WaterTerrainColliderSet>,
        chunk_id: IVec3,
    ) -> Option<Self> {
        let origin_ws = config.collider.min_ws;
        let extent_ws = config.collider.extent();
        if !extent_ws.is_finite() || extent_ws.min_element() <= 0.0 {
            return None;
        }

        let dx = (extent_ws.x / config.grid_dim.x as f32)
            .max(extent_ws.y / config.grid_dim.y as f32)
            .max(extent_ws.z / config.grid_dim.z as f32);
        if dx <= 0.0 || !dx.is_finite() {
            return None;
        }

        let inv_dx = dx.recip();
        let (min_node, max_node_exclusive) =
            terrain_grid_cache_range_for_chunk_parts(origin_ws, inv_dx, config.grid_dim, chunk_id)?;
        let terrain_chunk_count = terrain.as_ref().map_or(0, |terrain| terrain.chunks.len());
        let near_surface_band = dx * (config.terrain_collision_margin_cells.max(0.0) + 2.0);
        Some(Self {
            chunk_id,
            terrain_chunk_count,
            terrain,
            origin_ws,
            grid_dim: config.grid_dim,
            dx,
            min_node,
            max_node_exclusive,
            near_surface_band,
        })
    }

    pub fn chunk_id(&self) -> IVec3 {
        self.chunk_id
    }

    pub fn terrain_chunk_count(&self) -> usize {
        self.terrain_chunk_count
    }

    pub fn grid_dim(&self) -> UVec3 {
        self.grid_dim
    }

    pub fn min_node(&self) -> UVec3 {
        self.min_node
    }

    pub fn max_node_exclusive(&self) -> UVec3 {
        self.max_node_exclusive
    }

    pub fn near_surface_band(&self) -> f32 {
        self.near_surface_band
    }

    pub fn dx(&self) -> f32 {
        self.dx
    }

    pub fn node_count(&self) -> usize {
        terrain_cache_range_node_count(self.min_node, self.max_node_exclusive)
    }
}

#[derive(Debug)]
pub struct WaterTerrainCachePatch {
    chunk_id: IVec3,
    terrain_chunk_count: usize,
    grid_dim: UVec3,
    dx: f32,
    min_node: UVec3,
    max_node_exclusive: UVec3,
    near_surface_band: f32,
    samples: Vec<WaterTerrainGridSample>,
    stats: WaterTerrainCacheRebuildStats,
    build_ms: f32,
}

impl WaterTerrainCachePatch {
    pub fn chunk_id(&self) -> IVec3 {
        self.chunk_id
    }

    pub fn terrain_chunk_count(&self) -> usize {
        self.terrain_chunk_count
    }

    pub fn grid_dim(&self) -> UVec3 {
        self.grid_dim
    }

    pub fn min_node(&self) -> UVec3 {
        self.min_node
    }

    pub fn max_node_exclusive(&self) -> UVec3 {
        self.max_node_exclusive
    }

    pub fn near_surface_band(&self) -> f32 {
        self.near_surface_band
    }

    pub fn dx(&self) -> f32 {
        self.dx
    }

    pub fn stats(&self) -> WaterTerrainCacheRebuildStats {
        self.stats
    }

    pub fn build_ms(&self) -> f32 {
        self.build_ms
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaterTerrainCacheRebuildStats {
    pub node_count: usize,
    pub has_sdf_count: usize,
    pub near_surface_count: usize,
    pub normal_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WaterTerrainCacheApplyReport {
    pub chunk_id: IVec3,
    pub terrain_chunk_count: usize,
    pub grid_dim: UVec3,
    pub min_node: UVec3,
    pub max_node_exclusive: UVec3,
    pub node_count: usize,
    pub has_sdf_count: usize,
    pub near_surface_count: usize,
    pub normal_count: usize,
    pub near_surface_band: f32,
    pub dx: f32,
    pub build_ms: f32,
    pub apply_ms: f32,
}

pub fn build_terrain_grid_cache_patch(
    request: WaterTerrainCacheBuildRequest,
) -> WaterTerrainCachePatch {
    let build_start = Instant::now();
    let mut samples = Vec::with_capacity(request.node_count());
    let stats = build_terrain_grid_cache_samples(
        request.terrain.as_ref(),
        request.origin_ws,
        request.dx,
        request.min_node,
        request.max_node_exclusive,
        request.near_surface_band,
        |sample| samples.push(sample),
    );

    WaterTerrainCachePatch {
        chunk_id: request.chunk_id,
        terrain_chunk_count: request.terrain_chunk_count,
        grid_dim: request.grid_dim,
        dx: request.dx,
        min_node: request.min_node,
        max_node_exclusive: request.max_node_exclusive,
        near_surface_band: request.near_surface_band,
        samples,
        stats,
        build_ms: build_start.elapsed().as_secs_f32() * 1000.0,
    }
}

fn build_terrain_grid_cache_samples(
    terrain: Option<&WaterTerrainColliderSet>,
    origin_ws: Vec3,
    dx: f32,
    min_node: UVec3,
    max_node_exclusive: UVec3,
    near_surface_band: f32,
    mut push_sample: impl FnMut(WaterTerrainGridSample),
) -> WaterTerrainCacheRebuildStats {
    let mut stats = WaterTerrainCacheRebuildStats::default();

    for z in min_node.z..max_node_exclusive.z {
        for y in min_node.y..max_node_exclusive.y {
            for x in min_node.x..max_node_exclusive.x {
                let mut sample = WaterTerrainGridSample::default();
                if let Some(terrain) = terrain {
                    let node_world = origin_ws + Vec3::new(x as f32, y as f32, z as f32) * dx;
                    if let Some(sdf) = terrain.sample_sdf_ws(node_world) {
                        sample.sdf = sdf;
                        sample.has_sdf = true;
                        stats.has_sdf_count += 1;
                        sample.near_surface = sdf <= near_surface_band;
                        if sample.near_surface {
                            stats.near_surface_count += 1;
                            sample.normal = terrain.sample_normal_ws(node_world).unwrap_or(Vec3::Y);
                            stats.normal_count += 1;
                        }
                    }
                }
                push_sample(sample);
                stats.node_count += 1;
            }
        }
    }

    stats
}

fn terrain_cache_range_node_count(min_node: UVec3, max_node_exclusive: UVec3) -> usize {
    let extent = max_node_exclusive.saturating_sub(min_node);
    (extent.x as usize)
        .saturating_mul(extent.y as usize)
        .saturating_mul(extent.z as usize)
}


impl PondWaterSim {
    pub(crate) fn rebuild_terrain_grid_cache(&mut self) {
        let rebuild_start = Instant::now();
        let near_surface_band = self.terrain_grid_near_surface_band();
        let terrain_chunk_count = self.terrain.as_ref().map_or(0, |terrain| terrain.chunks.len());
        let grid_dim = self.grid_dim;
        let node_count = self.grid.len();

        self.ensure_terrain_grid_cache_len();
        let stats = self.rebuild_terrain_grid_cache_range(UVec3::ZERO, grid_dim, near_surface_band);

        log::info!(
            "[WATER][TERRAIN_CACHE] rebuilt grid cache chunks={} grid {:?} nodes={} has_sdf={} near_surface={} normals={} band={:.5} dx={:.5} total_ms={:.2}",
            terrain_chunk_count,
            grid_dim,
            node_count,
            stats.has_sdf_count,
            stats.near_surface_count,
            stats.normal_count,
            near_surface_band,
            self.dx,
            rebuild_start.elapsed().as_secs_f32() * 1000.0,
        );
    }

    pub fn terrain_grid_cache_build_request_for_chunk(
        &self,
        chunk_id: IVec3,
    ) -> Option<WaterTerrainCacheBuildRequest> {
        let (min_node, max_node_exclusive) = self.terrain_grid_cache_range_for_chunk(chunk_id)?;
        Some(WaterTerrainCacheBuildRequest {
            chunk_id,
            terrain_chunk_count: self.terrain.as_ref().map_or(0, |terrain| terrain.chunks.len()),
            terrain: self.terrain.clone(),
            origin_ws: self.origin_ws,
            grid_dim: self.grid_dim,
            dx: self.dx,
            min_node,
            max_node_exclusive,
            near_surface_band: self.terrain_grid_near_surface_band(),
        })
    }

    pub fn apply_terrain_grid_cache_patch(
        &mut self,
        patch: WaterTerrainCachePatch,
    ) -> Option<WaterTerrainCacheApplyReport> {
        let apply_start = Instant::now();
        if patch.grid_dim != self.grid_dim {
            log::warn!(
                "[WATER][TERRAIN_CACHE] discarded worker grid cache patch chunk={:?} grid {:?} current_grid {:?}: grid changed",
                patch.chunk_id,
                patch.grid_dim,
                self.grid_dim,
            );
            return None;
        }

        self.ensure_terrain_grid_cache_len();
        let expected_samples = terrain_cache_range_node_count(
            patch.min_node,
            patch.max_node_exclusive,
        );
        if patch.samples.len() != expected_samples {
            log::warn!(
                "[WATER][TERRAIN_CACHE] discarded worker grid cache patch chunk={:?} range {:?}..{:?}: sample count {} expected {}",
                patch.chunk_id,
                patch.min_node,
                patch.max_node_exclusive,
                patch.samples.len(),
                expected_samples,
            );
            return None;
        }

        let mut sample_idx = 0usize;
        for z in patch.min_node.z..patch.max_node_exclusive.z {
            for y in patch.min_node.y..patch.max_node_exclusive.y {
                for x in patch.min_node.x..patch.max_node_exclusive.x {
                    let idx = grid_index_dims(self.grid_dim, x, y, z);
                    self.terrain_grid[idx] = patch.samples[sample_idx];
                    sample_idx += 1;
                }
            }
        }

        Some(WaterTerrainCacheApplyReport {
            chunk_id: patch.chunk_id,
            terrain_chunk_count: patch.terrain_chunk_count,
            grid_dim: patch.grid_dim,
            min_node: patch.min_node,
            max_node_exclusive: patch.max_node_exclusive,
            node_count: patch.stats.node_count,
            has_sdf_count: patch.stats.has_sdf_count,
            near_surface_count: patch.stats.near_surface_count,
            normal_count: patch.stats.normal_count,
            near_surface_band: patch.near_surface_band,
            dx: patch.dx,
            build_ms: patch.build_ms,
            apply_ms: apply_start.elapsed().as_secs_f32() * 1000.0,
        })
    }

    pub fn rebuild_terrain_grid_cache_for_chunk(&mut self, chunk_id: IVec3) {
        let rebuild_start = Instant::now();
        let near_surface_band = self.terrain_grid_near_surface_band();
        let terrain_chunk_count = self.terrain.as_ref().map_or(0, |terrain| terrain.chunks.len());
        let grid_dim = self.grid_dim;

        self.ensure_terrain_grid_cache_len();
        let Some((min_node, max_node_exclusive)) = self.terrain_grid_cache_range_for_chunk(chunk_id)
        else {
            log::info!(
                "[WATER][TERRAIN_CACHE] skipped grid cache region chunk={:?} chunks={} grid {:?} outside=true total_ms={:.2}",
                chunk_id,
                terrain_chunk_count,
                grid_dim,
                rebuild_start.elapsed().as_secs_f32() * 1000.0,
            );
            return;
        };

        let stats = self.rebuild_terrain_grid_cache_range(
            min_node,
            max_node_exclusive,
            near_surface_band,
        );

        log::info!(
            "[WATER][TERRAIN_CACHE] rebuilt grid cache region chunk={:?} chunks={} grid {:?} range {:?}..{:?} nodes={} has_sdf={} near_surface={} normals={} band={:.5} dx={:.5} total_ms={:.2}",
            chunk_id,
            terrain_chunk_count,
            grid_dim,
            min_node,
            max_node_exclusive,
            stats.node_count,
            stats.has_sdf_count,
            stats.near_surface_count,
            stats.normal_count,
            near_surface_band,
            self.dx,
            rebuild_start.elapsed().as_secs_f32() * 1000.0,
        );
    }

    pub fn invalidate_terrain_grid_cache_for_chunk(&mut self, chunk_id: IVec3) {
        let invalidate_start = Instant::now();
        let terrain_chunk_count = self.terrain.as_ref().map_or(0, |terrain| terrain.chunks.len());
        let grid_dim = self.grid_dim;

        self.ensure_terrain_grid_cache_len();
        let Some((min_node, max_node_exclusive)) = self.terrain_grid_cache_range_for_chunk(chunk_id)
        else {
            log::debug!(
                "[WATER][TERRAIN_CACHE] skipped invalidating grid cache region chunk={:?} chunks={} grid {:?} outside=true total_ms={:.2}",
                chunk_id,
                terrain_chunk_count,
                grid_dim,
                invalidate_start.elapsed().as_secs_f32() * 1000.0,
            );
            return;
        };

        let mut node_count = 0usize;
        for z in min_node.z..max_node_exclusive.z {
            for y in min_node.y..max_node_exclusive.y {
                for x in min_node.x..max_node_exclusive.x {
                    let idx = grid_index_dims(grid_dim, x, y, z);
                    self.terrain_grid[idx] = WaterTerrainGridSample::default();
                    node_count += 1;
                }
            }
        }

        log::debug!(
            "[WATER][TERRAIN_CACHE] invalidated grid cache region chunk={:?} chunks={} grid {:?} range {:?}..{:?} nodes={} total_ms={:.2}",
            chunk_id,
            terrain_chunk_count,
            grid_dim,
            min_node,
            max_node_exclusive,
            node_count,
            invalidate_start.elapsed().as_secs_f32() * 1000.0,
        );
    }

    fn ensure_terrain_grid_cache_len(&mut self) {
        let node_count = self.grid.len();
        if self.terrain_grid.len() != node_count {
            self.terrain_grid = vec![WaterTerrainGridSample::default(); node_count];
        }
    }

    fn terrain_grid_near_surface_band(&self) -> f32 {
        // Cache a conservative narrow band around terrain. Hot loops use this
        // cheap water-grid SDF to skip exact collider queries for particles and
        // grid nodes that are clearly away from solids.
        self.terrain_collision_margin() + self.dx * 2.0
    }

    fn terrain_grid_cache_range_for_chunk(&self, chunk_id: IVec3) -> Option<(UVec3, UVec3)> {
        terrain_grid_cache_range_for_chunk_parts(
            self.origin_ws,
            self.inv_dx,
            self.grid_dim,
            chunk_id,
        )
    }

    fn rebuild_terrain_grid_cache_range(
        &mut self,
        min_node: UVec3,
        max_node_exclusive: UVec3,
        near_surface_band: f32,
    ) -> WaterTerrainCacheRebuildStats {
        let terrain = self.terrain.as_ref();
        let origin_ws = self.origin_ws;
        let grid_dim = self.grid_dim;
        let dx = self.dx;
        let mut stats = WaterTerrainCacheRebuildStats::default();

        for z in min_node.z..max_node_exclusive.z {
            for y in min_node.y..max_node_exclusive.y {
                for x in min_node.x..max_node_exclusive.x {
                    let idx = grid_index_dims(grid_dim, x, y, z);
                    let mut sample = WaterTerrainGridSample::default();
                    if let Some(terrain) = terrain {
                        let node_world = origin_ws + Vec3::new(x as f32, y as f32, z as f32) * dx;
                        if let Some(sdf) = terrain.sample_sdf_ws(node_world) {
                            sample.sdf = sdf;
                            sample.has_sdf = true;
                            stats.has_sdf_count += 1;
                            sample.near_surface = sdf <= near_surface_band;
                            if sample.near_surface {
                                stats.near_surface_count += 1;
                                sample.normal =
                                    terrain.sample_normal_ws(node_world).unwrap_or(Vec3::Y);
                                stats.normal_count += 1;
                            }
                        }
                    }
                    self.terrain_grid[idx] = sample;
                    stats.node_count += 1;
                }
            }
        }

        stats
    }

}

fn terrain_grid_cache_range_for_chunk_parts(
    origin_ws: Vec3,
    inv_dx: f32,
    grid_dim: UVec3,
    chunk_id: IVec3,
) -> Option<(UVec3, UVec3)> {
    const HALO_CELLS: i32 = 1;

    if !origin_ws.is_finite() || inv_dx <= 0.0 || !inv_dx.is_finite() {
        return None;
    }

    let min_ws = chunk_id.as_vec3();
    let max_ws = min_ws + Vec3::ONE;
    let min_grid = ((min_ws - origin_ws) * inv_dx).floor().as_ivec3()
        - IVec3::splat(HALO_CELLS);
    let max_grid = ((max_ws - origin_ws) * inv_dx).ceil().as_ivec3()
        + IVec3::splat(HALO_CELLS + 1);
    let grid_dim_i = grid_dim.as_ivec3();
    let min_node = min_grid.max(IVec3::ZERO).min(grid_dim_i);
    let max_node_exclusive = max_grid.max(IVec3::ZERO).min(grid_dim_i);
    if min_node.cmpge(max_node_exclusive).any() {
        return None;
    }
    Some((min_node.as_uvec3(), max_node_exclusive.as_uvec3()))
}

