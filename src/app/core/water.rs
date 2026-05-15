use super::App;
use glam::{IVec3, UVec3, Vec3};
use re_flora_water::{WaterTerrainColliderChunk, WaterTerrainColliderSet};
use std::{cmp::Ordering, collections::BinaryHeap, time::Instant};

const WATER_TERRAIN_COLLIDER_DIM: UVec3 = UVec3::new(32, 32, 32);
const WATER_TERRAIN_SINGLE_CHUNK_ID: IVec3 = IVec3::new(1, 0, 1);
const WATER_TERRAIN_CHUNK_QUERY_EPSILON_WS: f32 = 1.0 / 4096.0;
const WATER_TERRAIN_SURFACE_SKIP_WS: f32 = 2.0 / 256.0;
const WATER_TERRAIN_MAX_VERTICAL_CROSSINGS: usize = 64;

impl App {
    pub(super) fn water_terrain_sphere_overlaps(&self, center_ws: Vec3, radius_ws: f32) -> bool {
        let (min_ws, max_ws) = self.water_terrain_active_bounds_ws();
        sphere_aabb_overlaps(center_ws, radius_ws.max(0.0), min_ws, max_ws)
    }

    pub(super) fn invalidate_water_terrain_collider_for_overlapping_edit(&mut self) {
        if self.water_terrain_initialized {
            log::info!(
                "[WATER][TERRAIN] invalidated by overlapping terrain edit; preserving previous collider until refresh"
            );
        }
        // Preserve the previous valid collider in the sim while terrain/CPU-cache work settles.
        self.water_terrain_initialized = false;
    }

    pub(super) fn water_terrain_refresh_ready(&mut self) -> bool {
        if !self.deferred_chunk_rebuilds_idle() {
            log::debug!("[WATER][TERRAIN] refresh deferred until terrain rebuild queue is idle");
            return false;
        }

        self.contree_builder
            .poll_cpu_chunk_cache_jobs(self.tracer.camera_position(), super::VOXEL_DIM_PER_CHUNK);
        if !self.contree_builder.cpu_chunk_cache_jobs_idle() {
            log::debug!("[WATER][TERRAIN] refresh deferred until CPU terrain cache jobs finish");
            return false;
        }

        let chunk_id = WATER_TERRAIN_SINGLE_CHUNK_ID;
        let Some(chunk_idx) = water_terrain_chunk_id_to_uvec3(chunk_id) else {
            log::warn!("[WATER][TERRAIN] invalid target chunk id {chunk_id:?}");
            return false;
        };
        if !self.contree_builder.has_cpu_chunk_cache(chunk_idx) {
            log::debug!(
                "[WATER][TERRAIN] refresh deferred until CPU terrain cache for chunk {:?} is ready",
                chunk_id,
            );
            return false;
        }

        true
    }

    fn water_terrain_active_bounds_ws(&self) -> (Vec3, Vec3) {
        self.water_sim
            .terrain_collider_set()
            .and_then(WaterTerrainColliderSet::bounds_ws)
            .unwrap_or_else(|| self.water_terrain_target_bounds_ws())
    }

    fn water_terrain_target_bounds_ws(&self) -> (Vec3, Vec3) {
        water_terrain_chunk_bounds_ws(WATER_TERRAIN_SINGLE_CHUNK_ID)
    }

    pub(super) fn refresh_water_terrain_collider(&mut self) -> bool {
        let build_start = Instant::now();
        let chunk_id = WATER_TERRAIN_SINGLE_CHUNK_ID;
        let (bounds_min_ws, bounds_max_ws) = water_terrain_chunk_bounds_ws(chunk_id);
        let sample_count = (WATER_TERRAIN_COLLIDER_DIM.x as usize)
            * (WATER_TERRAIN_COLLIDER_DIM.y as usize)
            * (WATER_TERRAIN_COLLIDER_DIM.z as usize);

        let source = "surface-contree-vertical-parity";
        let (solid_samples, solid_stats) = self.water_terrain_solid_samples(
            WATER_TERRAIN_COLLIDER_DIM,
            bounds_min_ws,
            bounds_max_ws,
        );
        let solid_sample_count = solid_samples.iter().filter(|&&solid| solid).count();
        if solid_sample_count == 0 {
            log::warn!(
                "[WATER][TERRAIN] skipped collider chunk {:?}: no 3D terrain solid samples were found",
                chunk_id,
            );
            return false;
        }

        let sdf_ws = signed_distance_from_solid_samples(
            WATER_TERRAIN_COLLIDER_DIM,
            bounds_min_ws,
            bounds_max_ws,
            &solid_samples,
        );

        debug_assert_eq!(sdf_ws.len(), sample_count);
        let min_sdf = sdf_ws.iter().copied().fold(f32::INFINITY, f32::min);
        let max_sdf = sdf_ws.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let revision = self.next_water_terrain_revision();

        self.water_sim
            .set_terrain_collider_set(WaterTerrainColliderSet::from_chunk(
                WaterTerrainColliderChunk {
                    chunk_id,
                    dim: WATER_TERRAIN_COLLIDER_DIM,
                    sdf_ws,
                    revision,
                },
            ));

        let center_probe = (bounds_min_ws + bounds_max_ws) * 0.5;
        let center_sdf = self
            .water_sim
            .terrain_collider_set()
            .and_then(|set| set.sample_sdf_ws(center_probe));
        log::info!(
            "[WATER][TERRAIN] built collider chunk {:?} rev {} dim {:?} bounds {:?}..{:?} sdf {:.3}..{:.3} solid_samples {}/{} source {} columns_with_hits {}/{} crossings total={} max={} direct_surface_samples={} center_sdf={:?} build_ms={:.2}",
            chunk_id,
            revision,
            WATER_TERRAIN_COLLIDER_DIM,
            bounds_min_ws,
            bounds_max_ws,
            min_sdf,
            max_sdf,
            solid_sample_count,
            sample_count,
            source,
            solid_stats.columns_with_hits,
            solid_stats.columns,
            solid_stats.total_crossings,
            solid_stats.max_crossings,
            solid_stats.direct_surface_samples,
            center_sdf,
            build_start.elapsed().as_secs_f32() * 1000.0,
        );
        true
    }

    fn next_water_terrain_revision(&self) -> u64 {
        self.water_sim
            .terrain_collider_set()
            .and_then(|set| set.chunks.values().map(|chunk| chunk.revision).max())
            .unwrap_or(0)
            + 1
    }

    fn water_terrain_solid_samples(
        &self,
        dim: UVec3,
        bounds_min_ws: Vec3,
        bounds_max_ws: Vec3,
    ) -> (Vec<bool>, WaterTerrainSolidSampleStats) {
        let mut solid = vec![false; grid_len(dim)];
        let mut stats = WaterTerrainSolidSampleStats::default();
        let world_max_y = super::CHUNK_DIM.y as f32;

        for z in 0..dim.z {
            for x in 0..dim.x {
                let column_point_ws =
                    grid_sample_position(bounds_min_ws, bounds_max_ws, dim, x, 0, z);
                let column_query_ws =
                    chunk_local_query_position(column_point_ws, bounds_min_ws, bounds_max_ws);
                let crossings_y = self.water_terrain_vertical_surface_crossings_y(
                    column_query_ws.x,
                    column_query_ws.z,
                    bounds_min_ws.y,
                    world_max_y,
                );
                stats.columns += 1;
                stats.total_crossings += crossings_y.len();
                stats.max_crossings = stats.max_crossings.max(crossings_y.len());
                if !crossings_y.is_empty() {
                    stats.columns_with_hits += 1;
                }

                for y in 0..dim.y {
                    let point_ws = grid_sample_position(bounds_min_ws, bounds_max_ws, dim, x, y, z);
                    let query_point_ws =
                        chunk_local_query_position(point_ws, bounds_min_ws, bounds_max_ws);
                    let direct_surface = self.query_terrain_solid_cpu(query_point_ws);
                    if direct_surface {
                        stats.direct_surface_samples += 1;
                    }
                    let is_solid = direct_surface
                        || solid_from_descending_vertical_crossings(query_point_ws.y, &crossings_y);
                    solid[grid_index(dim, x, y, z)] = is_solid;
                }
            }
        }

        (solid, stats)
    }

    fn water_terrain_vertical_surface_crossings_y(
        &self,
        x_ws: f32,
        z_ws: f32,
        min_y_ws: f32,
        max_y_ws: f32,
    ) -> Vec<f32> {
        let mut crossings_y = Vec::new();
        let mut origin = Vec3::new(x_ws, max_y_ws + WATER_TERRAIN_SURFACE_SKIP_WS, z_ws);
        for _ in 0..WATER_TERRAIN_MAX_VERTICAL_CROSSINGS {
            let Some(hit) = self.query_terrain_ray_cpu(origin, Vec3::NEG_Y) else {
                break;
            };
            if hit.y < min_y_ws - WATER_TERRAIN_SURFACE_SKIP_WS {
                break;
            }
            if hit.y <= max_y_ws + WATER_TERRAIN_SURFACE_SKIP_WS {
                if crossings_y
                    .last()
                    .is_none_or(|last_y| last_y - hit.y > WATER_TERRAIN_SURFACE_SKIP_WS)
                {
                    crossings_y.push(hit.y);
                }
            }
            origin = Vec3::new(x_ws, hit.y - WATER_TERRAIN_SURFACE_SKIP_WS, z_ws);
        }
        crossings_y
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct WaterTerrainSolidSampleStats {
    columns: usize,
    columns_with_hits: usize,
    total_crossings: usize,
    max_crossings: usize,
    direct_surface_samples: usize,
}

fn water_terrain_chunk_bounds_ws(chunk_id: IVec3) -> (Vec3, Vec3) {
    let min_ws = chunk_id.as_vec3();
    (min_ws, min_ws + Vec3::ONE)
}

fn water_terrain_chunk_id_to_uvec3(chunk_id: IVec3) -> Option<UVec3> {
    if chunk_id.cmpge(IVec3::ZERO).all() {
        Some(chunk_id.as_uvec3())
    } else {
        None
    }
}

fn chunk_local_query_position(point_ws: Vec3, bounds_min_ws: Vec3, bounds_max_ws: Vec3) -> Vec3 {
    point_ws.clamp(
        bounds_min_ws,
        bounds_max_ws - Vec3::splat(WATER_TERRAIN_CHUNK_QUERY_EPSILON_WS),
    )
}

fn solid_from_descending_vertical_crossings(point_y: f32, crossings_y: &[f32]) -> bool {
    crossings_y
        .iter()
        .filter(|&&crossing_y| crossing_y + WATER_TERRAIN_SURFACE_SKIP_WS >= point_y)
        .count()
        % 2
        == 1
}

fn sphere_aabb_overlaps(center: Vec3, radius: f32, min: Vec3, max: Vec3) -> bool {
    let closest = center.clamp(min, max);
    (center - closest).length_squared() <= radius.max(0.0) * radius.max(0.0)
}

fn signed_distance_from_solid_samples(
    dim: UVec3,
    bounds_min_ws: Vec3,
    bounds_max_ws: Vec3,
    solid: &[bool],
) -> Vec<f32> {
    assert!(dim.x >= 2 && dim.y >= 2 && dim.z >= 2);
    assert_eq!(solid.len(), grid_len(dim));

    let cell_size = (bounds_max_ws - bounds_min_ws) / (dim - UVec3::ONE).as_vec3();
    let fallback_distance = (bounds_max_ws - bounds_min_ws)
        .length()
        .max(cell_size.length());
    let mut distance = vec![f32::INFINITY; solid.len()];
    let mut heap = BinaryHeap::new();

    for z in 0..dim.z {
        for y in 0..dim.y {
            for x in 0..dim.x {
                let idx = grid_index(dim, x, y, z);
                for_each_neighbor(dim, x, y, z, |nx, ny, nz, offset| {
                    let neighbor_idx = grid_index(dim, nx, ny, nz);
                    if solid[idx] != solid[neighbor_idx] {
                        let seed_distance = neighbor_step_distance(offset, cell_size) * 0.5;
                        distance[idx] = distance[idx].min(seed_distance);
                    }
                });
                if distance[idx].is_finite() {
                    heap.push(DistanceQueueEntry {
                        distance: distance[idx],
                        index: idx,
                    });
                }
            }
        }
    }

    if heap.is_empty() {
        return solid
            .iter()
            .map(|&is_solid| {
                if is_solid {
                    -fallback_distance
                } else {
                    fallback_distance
                }
            })
            .collect();
    }

    while let Some(entry) = heap.pop() {
        if entry.distance > distance[entry.index] + 1.0e-6 {
            continue;
        }

        let (x, y, z) = grid_coords(dim, entry.index);
        for_each_neighbor(dim, x, y, z, |nx, ny, nz, offset| {
            let neighbor_idx = grid_index(dim, nx, ny, nz);
            let next_distance = entry.distance + neighbor_step_distance(offset, cell_size);
            if next_distance < distance[neighbor_idx] {
                distance[neighbor_idx] = next_distance;
                heap.push(DistanceQueueEntry {
                    distance: next_distance,
                    index: neighbor_idx,
                });
            }
        });
    }

    distance
        .into_iter()
        .zip(solid.iter().copied())
        .map(|(distance, is_solid)| if is_solid { -distance } else { distance })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DistanceQueueEntry {
    distance: f32,
    index: usize,
}

impl Eq for DistanceQueueEntry {}

impl Ord for DistanceQueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.index.cmp(&self.index))
    }
}

impl PartialOrd for DistanceQueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn grid_sample_position(
    bounds_min_ws: Vec3,
    bounds_max_ws: Vec3,
    dim: UVec3,
    x: u32,
    y: u32,
    z: u32,
) -> Vec3 {
    let t = Vec3::new(
        x as f32 / (dim.x - 1) as f32,
        y as f32 / (dim.y - 1) as f32,
        z as f32 / (dim.z - 1) as f32,
    );
    bounds_min_ws + (bounds_max_ws - bounds_min_ws) * t
}

fn grid_len(dim: UVec3) -> usize {
    (dim.x as usize) * (dim.y as usize) * (dim.z as usize)
}

fn grid_index(dim: UVec3, x: u32, y: u32, z: u32) -> usize {
    ((z as usize * dim.y as usize + y as usize) * dim.x as usize) + x as usize
}

fn grid_coords(dim: UVec3, index: usize) -> (u32, u32, u32) {
    let x = index % dim.x as usize;
    let yz = index / dim.x as usize;
    let y = yz % dim.y as usize;
    let z = yz / dim.y as usize;
    (x as u32, y as u32, z as u32)
}

fn for_each_neighbor(dim: UVec3, x: u32, y: u32, z: u32, mut f: impl FnMut(u32, u32, u32, IVec3)) {
    for oz in -1..=1 {
        for oy in -1..=1 {
            for ox in -1..=1 {
                if ox == 0 && oy == 0 && oz == 0 {
                    continue;
                }
                let nx = x as i32 + ox;
                let ny = y as i32 + oy;
                let nz = z as i32 + oz;
                if nx >= 0
                    && ny >= 0
                    && nz >= 0
                    && nx < dim.x as i32
                    && ny < dim.y as i32
                    && nz < dim.z as i32
                {
                    f(nx as u32, ny as u32, nz as u32, IVec3::new(ox, oy, oz));
                }
            }
        }
    }
}

fn neighbor_step_distance(offset: IVec3, cell_size: Vec3) -> f32 {
    (offset.as_vec3() * cell_size).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_water_collider_chunk_matches_initial_pond_chunk() {
        let (min_ws, max_ws) = water_terrain_chunk_bounds_ws(WATER_TERRAIN_SINGLE_CHUNK_ID);

        assert_eq!(WATER_TERRAIN_SINGLE_CHUNK_ID, IVec3::new(1, 0, 1));
        assert_eq!(min_ws, Vec3::new(1.0, 0.0, 1.0));
        assert_eq!(max_ws, Vec3::new(2.0, 1.0, 2.0));
    }

    #[test]
    fn sphere_overlap_includes_shared_edges() {
        assert!(sphere_aabb_overlaps(
            Vec3::new(1.0, 0.5, 0.5),
            0.0,
            Vec3::ZERO,
            Vec3::ONE,
        ));
    }

    #[test]
    fn sphere_overlap_rejects_separated_bounds() {
        assert!(!sphere_aabb_overlaps(
            Vec3::new(2.1, 0.5, 0.5),
            0.5,
            Vec3::ZERO,
            Vec3::ONE,
        ));
    }

    #[test]
    fn chunk_local_query_position_keeps_max_face_inside_chunk() {
        let p = chunk_local_query_position(Vec3::ONE, Vec3::ZERO, Vec3::ONE);

        assert_eq!(p.x, 1.0 - WATER_TERRAIN_CHUNK_QUERY_EPSILON_WS);
        assert_eq!(p.y, 1.0 - WATER_TERRAIN_CHUNK_QUERY_EPSILON_WS);
        assert_eq!(p.z, 1.0 - WATER_TERRAIN_CHUNK_QUERY_EPSILON_WS);
    }

    #[test]
    fn chunk_id_conversion_rejects_negative_chunks() {
        assert_eq!(
            water_terrain_chunk_id_to_uvec3(IVec3::new(1, 0, 1)),
            Some(UVec3::new(1, 0, 1))
        );
        assert_eq!(water_terrain_chunk_id_to_uvec3(IVec3::new(-1, 0, 1)), None);
    }

    #[test]
    fn vertical_crossing_parity_fills_solid_volume() {
        let crossings_y = [0.5];

        assert!(!solid_from_descending_vertical_crossings(
            0.75,
            &crossings_y
        ));
        assert!(solid_from_descending_vertical_crossings(0.25, &crossings_y));
    }

    #[test]
    fn vertical_crossing_parity_preserves_cave_empty_space() {
        let crossings_y = [0.8, 0.6, 0.4, 0.1];

        assert!(!solid_from_descending_vertical_crossings(0.9, &crossings_y));
        assert!(solid_from_descending_vertical_crossings(0.7, &crossings_y));
        assert!(!solid_from_descending_vertical_crossings(0.5, &crossings_y));
        assert!(solid_from_descending_vertical_crossings(0.2, &crossings_y));
        assert!(!solid_from_descending_vertical_crossings(
            0.05,
            &crossings_y
        ));
    }

    #[test]
    fn signed_distance_marks_solid_negative_and_empty_positive() {
        let dim = UVec3::new(3, 3, 3);
        let mut solid = vec![false; grid_len(dim)];
        for z in 0..dim.z {
            for x in 0..dim.x {
                solid[grid_index(dim, x, 0, z)] = true;
            }
        }

        let sdf = signed_distance_from_solid_samples(dim, Vec3::ZERO, Vec3::ONE, &solid);

        assert!(sdf[grid_index(dim, 1, 0, 1)] < 0.0);
        assert!(sdf[grid_index(dim, 1, 2, 1)] > 0.0);
        assert!(sdf[grid_index(dim, 1, 1, 1)].abs() < sdf[grid_index(dim, 1, 2, 1)].abs());
    }
}
