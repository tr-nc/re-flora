use super::{App, WaterTerrainColliderRebuildRequest};
use glam::{IVec3, UVec3, Vec3};
use re_flora_water::WaterTerrainColliderChunk;
use std::{
    cmp::Ordering,
    collections::BinaryHeap,
    time::{Duration, Instant},
};

const WATER_TERRAIN_COLLIDER_DIM: UVec3 = UVec3::new(32, 32, 32);
const WATER_TERRAIN_SINGLE_CHUNK_ID: IVec3 = IVec3::new(1, 0, 1);
const WATER_TERRAIN_CHUNK_QUERY_EPSILON_WS: f32 = 1.0 / 4096.0;
const WATER_TERRAIN_SURFACE_SKIP_WS: f32 = 2.0 / 256.0;
const WATER_TERRAIN_MAX_VERTICAL_CROSSINGS: usize = 64;
const WATER_TERRAIN_COLLIDER_SOURCE: &str = "surface-contree-vertical-parity";
const WATER_TERRAIN_EDIT_DEBOUNCE: Duration = Duration::from_millis(300);

impl App {
    pub(super) fn enqueue_startup_water_terrain_collider_rebuild(&mut self) {
        let Some(chunk_id) = water_terrain_chunk_id_to_uvec3(WATER_TERRAIN_SINGLE_CHUNK_ID) else {
            log::warn!(
                "[WATER][TERRAIN] invalid startup collider chunk {:?}",
                WATER_TERRAIN_SINGLE_CHUNK_ID
            );
            return;
        };
        self.enqueue_deferred_water_terrain_collider_rebuild(chunk_id);
    }

    pub(super) fn enqueue_deferred_water_terrain_collider_rebuild(&mut self, chunk_id: UVec3) {
        let revision = self
            .deferred_water_terrain_collider_rebuilds
            .push(chunk_id, WaterTerrainColliderRebuildRequest);
        log::debug!(
            "[QUEUE][WATER_TERRAIN] enqueue chunk {:?} revision {} pending={} active={}",
            chunk_id,
            revision,
            self.deferred_water_terrain_collider_rebuilds.len(),
            self.deferred_water_terrain_collider_rebuilds.active_len(),
        );
    }

    pub(super) fn process_deferred_water_terrain_collider_rebuild(&mut self) {
        if !self.deferred_chunk_rebuilds_idle() {
            return;
        }
        if !self.contree_builder.cpu_chunk_cache_jobs_idle() {
            return;
        }
        if self.water_terrain_edit_recent_or_active() {
            return;
        }

        let focus = self.water_terrain_focus_ws();
        let Some(work) = self
            .deferred_water_terrain_collider_rebuilds
            .pop_nearest_to(focus, UVec3::ONE)
        else {
            return;
        };

        let chunk_key = work.chunk_id;
        let revision = work.revision;
        let Some(chunk_id) = water_terrain_chunk_work_key_to_id(chunk_key) else {
            log::warn!(
                "[WATER][TERRAIN] skipped invalid queued collider chunk {:?} rev {}",
                chunk_key,
                revision,
            );
            self.deferred_water_terrain_collider_rebuilds
                .complete(chunk_key, revision);
            return;
        };

        let Some(build) = self.build_water_terrain_collider_chunk(chunk_id, revision) else {
            self.deferred_water_terrain_collider_rebuilds
                .complete(chunk_key, revision);
            self.water_terrain_initialized = self.water_terrain_has_startup_collider();
            return;
        };

        let center_probe = (build.bounds_min_ws + build.bounds_max_ws) * 0.5;
        self.publish_water_terrain_collider_chunk(build.chunk);
        self.deferred_water_terrain_collider_rebuilds
            .complete(chunk_key, revision);

        let center_sdf = self
            .water_sim
            .terrain_collider_set()
            .and_then(|set| set.sample_sdf_ws(center_probe));
        log::info!(
            "[WATER][TERRAIN] built collider chunk {:?} rev {} dim {:?} bounds {:?}..{:?} sdf {:.3}..{:.3} solid_samples {}/{} source {} columns_with_hits {}/{} crossings total={} max={} direct_surface_samples={} center_sdf={:?} build_ms={:.2} queue_pending={}",
            chunk_id,
            revision,
            WATER_TERRAIN_COLLIDER_DIM,
            build.bounds_min_ws,
            build.bounds_max_ws,
            build.stats.min_sdf,
            build.stats.max_sdf,
            build.stats.solid_sample_count,
            build.stats.sample_count,
            WATER_TERRAIN_COLLIDER_SOURCE,
            build.stats.solid.columns_with_hits,
            build.stats.solid.columns,
            build.stats.solid.total_crossings,
            build.stats.solid.max_crossings,
            build.stats.solid.direct_surface_samples,
            center_sdf,
            build.stats.build_ms,
            self.deferred_water_terrain_collider_rebuilds.len(),
        );
    }

    fn build_water_terrain_collider_chunk(
        &self,
        chunk_id: IVec3,
        revision: u64,
    ) -> Option<WaterTerrainColliderBuild> {
        let build_start = Instant::now();
        let (bounds_min_ws, bounds_max_ws) = water_terrain_chunk_bounds_ws(chunk_id);
        let sample_count = (WATER_TERRAIN_COLLIDER_DIM.x as usize)
            * (WATER_TERRAIN_COLLIDER_DIM.y as usize)
            * (WATER_TERRAIN_COLLIDER_DIM.z as usize);

        let (solid_samples, solid_stats) = self.water_terrain_solid_samples(
            WATER_TERRAIN_COLLIDER_DIM,
            bounds_min_ws,
            bounds_max_ws,
        );
        let solid_sample_count = solid_samples.iter().filter(|&&solid| solid).count();
        if solid_sample_count == 0 {
            log::warn!(
                "[WATER][TERRAIN] skipped collider chunk {:?} rev {}: no 3D terrain solid samples were found build_ms={:.2}",
                chunk_id,
                revision,
                build_start.elapsed().as_secs_f32() * 1000.0,
            );
            return None;
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

        Some(WaterTerrainColliderBuild {
            chunk: WaterTerrainColliderChunk {
                chunk_id,
                dim: WATER_TERRAIN_COLLIDER_DIM,
                sdf_ws,
                revision,
            },
            bounds_min_ws,
            bounds_max_ws,
            stats: WaterTerrainColliderBuildStats {
                sample_count,
                solid_sample_count,
                min_sdf,
                max_sdf,
                solid: solid_stats,
                build_ms: build_start.elapsed().as_secs_f32() * 1000.0,
            },
        })
    }

    fn publish_water_terrain_collider_chunk(&mut self, chunk: WaterTerrainColliderChunk) {
        let should_stabilize_particles = water_terrain_chunk_strictly_overlaps_box(
            chunk.chunk_id,
            self.water_sim.config.collider.min_ws,
            self.water_sim.config.collider.max_ws,
        );
        self.water_sim
            .upsert_terrain_collider_chunk(chunk, should_stabilize_particles);
        self.water_terrain_initialized = self.water_terrain_has_startup_collider();
    }

    fn water_terrain_edit_recent_or_active(&self) -> bool {
        let editing_now = (self.is_shovel_selected()
            && self.shovel_dig_held
            && (self.left_mouse_held || self.right_mouse_held))
            || (self.is_staff_selected() && self.left_mouse_held)
            || (self.is_hoe_selected() && self.left_mouse_held);
        if editing_now {
            return true;
        }

        let now = Instant::now();
        [
            self.last_shovel_dig_time,
            self.last_shovel_place_time,
            self.last_staff_regen_time,
            self.last_hoe_trim_time,
        ]
        .into_iter()
        .flatten()
        .any(|last_edit| now.duration_since(last_edit) < WATER_TERRAIN_EDIT_DEBOUNCE)
    }

    fn water_terrain_focus_ws(&self) -> Vec3 {
        let bounds = self.water_sim.config.collider;
        bounds.min_ws + bounds.extent() * 0.5
    }

    fn water_terrain_has_startup_collider(&self) -> bool {
        self.water_sim
            .terrain_collider_set()
            .is_some_and(|set| set.chunks.contains_key(&WATER_TERRAIN_SINGLE_CHUNK_ID))
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

struct WaterTerrainColliderBuild {
    chunk: WaterTerrainColliderChunk,
    bounds_min_ws: Vec3,
    bounds_max_ws: Vec3,
    stats: WaterTerrainColliderBuildStats,
}

struct WaterTerrainColliderBuildStats {
    sample_count: usize,
    solid_sample_count: usize,
    min_sdf: f32,
    max_sdf: f32,
    solid: WaterTerrainSolidSampleStats,
    build_ms: f32,
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

fn water_terrain_chunk_strictly_overlaps_box(
    chunk_id: IVec3,
    box_min_ws: Vec3,
    box_max_ws: Vec3,
) -> bool {
    let (chunk_min_ws, chunk_max_ws) = water_terrain_chunk_bounds_ws(chunk_id);
    chunk_min_ws.x < box_max_ws.x
        && chunk_max_ws.x > box_min_ws.x
        && chunk_min_ws.y < box_max_ws.y
        && chunk_max_ws.y > box_min_ws.y
        && chunk_min_ws.z < box_max_ws.z
        && chunk_max_ws.z > box_min_ws.z
}

fn water_terrain_chunk_id_to_uvec3(chunk_id: IVec3) -> Option<UVec3> {
    if chunk_id.cmpge(IVec3::ZERO).all() {
        Some(chunk_id.as_uvec3())
    } else {
        None
    }
}

fn water_terrain_chunk_work_key_to_id(chunk_id: UVec3) -> Option<IVec3> {
    if chunk_id.cmple(UVec3::splat(i32::MAX as u32)).all() {
        Some(chunk_id.as_ivec3())
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

#[cfg(test)]
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
    fn water_chunk_work_key_round_trips_startup_chunk() {
        let chunk_key = water_terrain_chunk_id_to_uvec3(WATER_TERRAIN_SINGLE_CHUNK_ID).unwrap();

        assert_eq!(chunk_key, UVec3::new(1, 0, 1));
        assert_eq!(
            water_terrain_chunk_work_key_to_id(chunk_key),
            Some(WATER_TERRAIN_SINGLE_CHUNK_ID)
        );
    }

    #[test]
    fn water_chunk_work_key_rejects_overflowing_ids() {
        assert_eq!(
            water_terrain_chunk_work_key_to_id(UVec3::new(i32::MAX as u32 + 1, 0, 0)),
            None
        );
    }

    #[test]
    fn chunk_strict_overlap_accepts_startup_chunk() {
        assert!(water_terrain_chunk_strictly_overlaps_box(
            WATER_TERRAIN_SINGLE_CHUNK_ID,
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(2.0, 1.0, 2.0),
        ));
    }

    #[test]
    fn chunk_strict_overlap_rejects_only_touching_neighbor() {
        assert!(!water_terrain_chunk_strictly_overlaps_box(
            IVec3::new(1, 0, 2),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(2.0, 1.0, 2.0),
        ));
        assert!(!water_terrain_chunk_strictly_overlaps_box(
            IVec3::new(1, 1, 1),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(2.0, 1.0, 2.0),
        ));
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
