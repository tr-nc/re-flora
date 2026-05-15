use super::{App, WaterTerrainColliderRebuildRequest};
use crate::builder::{ContreeBuilder, ContreeCpuRayQuerySnapshot};
use glam::{IVec3, UVec3, Vec3};
use re_flora_terrain_collider::signed_distance_from_solid_samples;
use re_flora_water::WaterTerrainColliderChunk;
use std::{sync::mpsc, thread, time::Instant};

const WATER_TERRAIN_COLLIDER_DIM: UVec3 = UVec3::new(32, 32, 32);
const WATER_TERRAIN_SINGLE_CHUNK_ID: IVec3 = IVec3::new(1, 0, 1);
const WATER_TERRAIN_CHUNK_QUERY_EPSILON_WS: f32 = 1.0 / 4096.0;
const WATER_TERRAIN_SURFACE_SKIP_WS: f32 = 2.0 / 256.0;
const WATER_TERRAIN_MAX_VERTICAL_CROSSINGS: usize = 64;
const WATER_TERRAIN_COLLIDER_SOURCE: &str = "surface-contree-vertical-parity";

pub(super) struct WaterTerrainColliderWorkerJob {
    chunk_key: UVec3,
    chunk_id: IVec3,
    revision: u64,
    query_snapshot: ContreeCpuRayQuerySnapshot,
}

pub(super) struct WaterTerrainColliderWorkerResult {
    chunk_key: UVec3,
    chunk_id: IVec3,
    revision: u64,
    build: Option<WaterTerrainColliderBuild>,
}

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

    pub(super) fn mark_water_terrain_source_chunk_dirty(&mut self, chunk_id: UVec3) {
        let was_new = self.pending_water_terrain_source_chunks.insert(chunk_id);
        log::debug!(
            "[QUEUE][WATER_TERRAIN] await CPU source chunk {:?} new={} pending_sources={}",
            chunk_id,
            was_new,
            self.pending_water_terrain_source_chunks.len(),
        );
    }

    pub(super) fn process_water_terrain_source_updates(&mut self) {
        for update in self.contree_builder.take_cpu_chunk_source_updates() {
            if !self
                .pending_water_terrain_source_chunks
                .remove(&update.chunk_idx)
            {
                continue;
            }

            log::debug!(
                "[QUEUE][WATER_TERRAIN] CPU source ready chunk {:?} source_rev={} present={} pending_sources={}",
                update.chunk_idx,
                update.revision,
                update.is_present,
                self.pending_water_terrain_source_chunks.len(),
            );
            self.enqueue_deferred_water_terrain_collider_rebuild(update.chunk_idx);
        }
    }

    pub(super) fn process_deferred_water_terrain_collider_rebuild(&mut self) {
        self.publish_completed_water_terrain_collider_rebuilds();
        self.try_submit_next_water_terrain_collider_rebuild();
    }

    pub(super) fn spawn_water_terrain_collider_worker() -> (
        mpsc::Sender<WaterTerrainColliderWorkerJob>,
        mpsc::Receiver<WaterTerrainColliderWorkerResult>,
    ) {
        let (job_tx, job_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        thread::spawn(move || loop {
            let job: WaterTerrainColliderWorkerJob = match job_rx.recv() {
                Ok(job) => job,
                Err(_) => break,
            };

            let build =
                build_water_terrain_collider_chunk(&job.query_snapshot, job.chunk_id, job.revision);
            let result = WaterTerrainColliderWorkerResult {
                chunk_key: job.chunk_key,
                chunk_id: job.chunk_id,
                revision: job.revision,
                build,
            };
            if result_tx.send(result).is_err() {
                break;
            }
        });

        (job_tx, result_rx)
    }

    fn try_submit_next_water_terrain_collider_rebuild(&mut self) {
        if self.water_terrain_collider_build_inflight {
            return;
        }

        let focus = self.water_terrain_focus_ws();
        let contree_builder = &self.contree_builder;
        let Some(work) = self
            .deferred_water_terrain_collider_rebuilds
            .pop_nearest_to_if(focus, UVec3::ONE, |chunk_key| {
                water_terrain_chunk_work_key_to_id(chunk_key).is_none()
                    || water_terrain_chunk_query_source_ready(contree_builder, chunk_key)
            })
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

        let query_snapshot = self.contree_builder.cpu_ray_query_snapshot();
        let job = WaterTerrainColliderWorkerJob {
            chunk_key,
            chunk_id,
            revision,
            query_snapshot,
        };
        self.water_terrain_collider_build_inflight = true;
        if let Err(err) = self.water_terrain_collider_job_tx.send(job) {
            log::error!(
                "[WATER][TERRAIN] failed to submit collider build chunk {:?} rev {}: {}",
                chunk_id,
                revision,
                err,
            );
            self.water_terrain_collider_build_inflight = false;
            self.deferred_water_terrain_collider_rebuilds
                .complete(chunk_key, revision);
            return;
        }

        log::debug!(
            "[QUEUE][WATER_TERRAIN] submit chunk {:?} revision {} pending={} active={}",
            chunk_id,
            revision,
            self.deferred_water_terrain_collider_rebuilds.len(),
            self.deferred_water_terrain_collider_rebuilds.active_len(),
        );
    }

    fn publish_completed_water_terrain_collider_rebuilds(&mut self) {
        while let Ok(result) = self.water_terrain_collider_result_rx.try_recv() {
            self.water_terrain_collider_build_inflight = false;
            let is_latest = self
                .deferred_water_terrain_collider_rebuilds
                .is_latest_revision(result.chunk_key, result.revision);

            if let Some(build) = result.build {
                let WaterTerrainColliderBuild {
                    chunk,
                    bounds_min_ws,
                    bounds_max_ws,
                    stats,
                } = build;
                let center_probe = (bounds_min_ws + bounds_max_ws) * 0.5;
                self.publish_water_terrain_collider_chunk(chunk);
                let center_sdf = self
                    .water_sim
                    .terrain_collider_set()
                    .and_then(|set| set.sample_sdf_ws(center_probe));
                log::info!(
                    "[WATER][TERRAIN] built collider chunk {:?} rev {} latest={} dim {:?} bounds {:?}..{:?} sdf {:.3}..{:.3} sdf_hash {:016x} solid_samples {}/{} source {} columns_with_hits {}/{} crossings total={} max={} direct_surface_samples={} center_sdf={:?} build_ms={:.2} phases solid={:.2} count={:.2} sdf={:.2} stats={:.2} queue_pending={}",
                    result.chunk_id,
                    result.revision,
                    is_latest,
                    WATER_TERRAIN_COLLIDER_DIM,
                    bounds_min_ws,
                    bounds_max_ws,
                    stats.min_sdf,
                    stats.max_sdf,
                    stats.sdf_hash,
                    stats.solid_sample_count,
                    stats.sample_count,
                    WATER_TERRAIN_COLLIDER_SOURCE,
                    stats.solid.columns_with_hits,
                    stats.solid.columns,
                    stats.solid.total_crossings,
                    stats.solid.max_crossings,
                    stats.solid.direct_surface_samples,
                    center_sdf,
                    stats.build_ms,
                    stats.solid_ms,
                    stats.count_ms,
                    stats.sdf_ms,
                    stats.stats_ms,
                    self.deferred_water_terrain_collider_rebuilds.len(),
                );
            } else if is_latest {
                self.water_terrain_initialized = self.water_terrain_has_startup_collider();
            }

            self.deferred_water_terrain_collider_rebuilds
                .complete(result.chunk_key, result.revision);
        }
    }

    fn publish_water_terrain_collider_chunk(&mut self, chunk: WaterTerrainColliderChunk) {
        let chunk_id = chunk.chunk_id;
        let already_had_chunk = self
            .water_sim
            .terrain_collider_set()
            .is_some_and(|set| set.chunks.contains_key(&chunk_id));
        let should_stabilize_particles = !already_had_chunk
            && water_terrain_chunk_strictly_overlaps_box(
                chunk_id,
                self.water_sim.config.collider.min_ws,
                self.water_sim.config.collider.max_ws,
            );
        self.water_sim
            .upsert_terrain_collider_chunk(chunk, should_stabilize_particles);
        self.water_terrain_initialized = self.water_terrain_has_startup_collider();
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
}

fn build_water_terrain_collider_chunk(
    query_snapshot: &ContreeCpuRayQuerySnapshot,
    chunk_id: IVec3,
    revision: u64,
) -> Option<WaterTerrainColliderBuild> {
    let build_start = Instant::now();
    let (bounds_min_ws, bounds_max_ws) = water_terrain_chunk_bounds_ws(chunk_id);
    let sample_count = (WATER_TERRAIN_COLLIDER_DIM.x as usize)
        * (WATER_TERRAIN_COLLIDER_DIM.y as usize)
        * (WATER_TERRAIN_COLLIDER_DIM.z as usize);

    let solid_start = Instant::now();
    let (solid_samples, solid_stats) = water_terrain_solid_samples(
        query_snapshot,
        WATER_TERRAIN_COLLIDER_DIM,
        bounds_min_ws,
        bounds_max_ws,
    );
    let solid_ms = solid_start.elapsed().as_secs_f32() * 1000.0;
    let count_start = Instant::now();
    let solid_sample_count = solid_samples.iter().filter(|&&solid| solid).count();
    let count_ms = count_start.elapsed().as_secs_f32() * 1000.0;
    if solid_sample_count == 0 {
        log::warn!(
            "[WATER][TERRAIN] skipped collider chunk {:?} rev {}: no 3D terrain solid samples were found build_ms={:.2}",
            chunk_id,
            revision,
            build_start.elapsed().as_secs_f32() * 1000.0,
        );
        return None;
    }

    let sdf_start = Instant::now();
    let sdf_ws = signed_distance_from_solid_samples(
        WATER_TERRAIN_COLLIDER_DIM,
        bounds_min_ws,
        bounds_max_ws,
        &solid_samples,
    );
    let sdf_ms = sdf_start.elapsed().as_secs_f32() * 1000.0;

    debug_assert_eq!(sdf_ws.len(), sample_count);
    let stats_start = Instant::now();
    let min_sdf = sdf_ws.iter().copied().fold(f32::INFINITY, f32::min);
    let max_sdf = sdf_ws.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sdf_hash = hash_sdf_samples(&sdf_ws);
    let stats_ms = stats_start.elapsed().as_secs_f32() * 1000.0;

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
            sdf_hash,
            solid_ms,
            count_ms,
            sdf_ms,
            stats_ms,
            build_ms: build_start.elapsed().as_secs_f32() * 1000.0,
        },
    })
}

fn water_terrain_solid_samples(
    query_snapshot: &ContreeCpuRayQuerySnapshot,
    dim: UVec3,
    bounds_min_ws: Vec3,
    bounds_max_ws: Vec3,
) -> (Vec<bool>, WaterTerrainSolidSampleStats) {
    let mut solid = vec![false; grid_len(dim)];
    let mut stats = WaterTerrainSolidSampleStats::default();
    let world_max_y = super::CHUNK_DIM.y as f32;

    for z in 0..dim.z {
        for x in 0..dim.x {
            let column_point_ws = grid_sample_position(bounds_min_ws, bounds_max_ws, dim, x, 0, z);
            let column_query_ws =
                chunk_local_query_position(column_point_ws, bounds_min_ws, bounds_max_ws);
            let crossings_y = water_terrain_vertical_surface_crossings_y(
                query_snapshot,
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
                let direct_surface = query_snapshot.query_terrain_occupancy_cpu(query_point_ws);
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
    query_snapshot: &ContreeCpuRayQuerySnapshot,
    x_ws: f32,
    z_ws: f32,
    min_y_ws: f32,
    max_y_ws: f32,
) -> Vec<f32> {
    let mut crossings_y = Vec::new();
    let mut origin = Vec3::new(x_ws, max_y_ws + WATER_TERRAIN_SURFACE_SKIP_WS, z_ws);
    for _ in 0..WATER_TERRAIN_MAX_VERTICAL_CROSSINGS {
        let Some(hit) = query_snapshot.query_terrain_ray_cpu(origin, Vec3::NEG_Y) else {
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
    sdf_hash: u64,
    solid_ms: f32,
    count_ms: f32,
    sdf_ms: f32,
    stats_ms: f32,
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

fn water_terrain_chunk_query_source_ready(
    contree_builder: &ContreeBuilder,
    chunk_key: UVec3,
) -> bool {
    water_terrain_chunk_query_dependency_keys(chunk_key)
        .into_iter()
        .all(|dependency| contree_builder.cpu_chunk_query_source_ready(dependency))
}

fn water_terrain_chunk_query_dependency_keys(chunk_key: UVec3) -> Vec<UVec3> {
    if chunk_key.x >= super::CHUNK_DIM.x
        || chunk_key.y >= super::CHUNK_DIM.y
        || chunk_key.z >= super::CHUNK_DIM.z
    {
        return Vec::new();
    }

    (chunk_key.y..super::CHUNK_DIM.y)
        .map(|y| UVec3::new(chunk_key.x, y, chunk_key.z))
        .collect()
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

fn hash_sdf_samples(sdf_ws: &[f32]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    sdf_ws.iter().fold(FNV_OFFSET, |hash, sdf| {
        (hash ^ sdf.to_bits() as u64).wrapping_mul(FNV_PRIME)
    })
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
    fn water_query_dependencies_include_same_vertical_column_above_chunk() {
        assert_eq!(
            water_terrain_chunk_query_dependency_keys(UVec3::new(1, 0, 1)),
            vec![UVec3::new(1, 0, 1), UVec3::new(1, 1, 1)]
        );
        assert_eq!(
            water_terrain_chunk_query_dependency_keys(UVec3::new(1, 1, 1)),
            vec![UVec3::new(1, 1, 1)]
        );
    }

    #[test]
    fn water_query_dependencies_ignore_out_of_world_chunks() {
        assert!(water_terrain_chunk_query_dependency_keys(UVec3::new(5, 0, 1)).is_empty());
        assert!(water_terrain_chunk_query_dependency_keys(UVec3::new(1, 2, 1)).is_empty());
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
