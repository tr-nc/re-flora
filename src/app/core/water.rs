use super::{App, WaterTerrainColliderRebuildRequest, VOXEL_DIM_PER_CHUNK};
use crate::app::cpu_solid_voxels::CpuSolidVoxelChunk;
use glam::{IVec3, UVec3, Vec3};
use re_flora_terrain_collider::signed_distance_from_solid_samples;
use re_flora_water::WaterTerrainColliderChunk;
use std::{sync::mpsc, thread, time::Instant};

const WATER_TERRAIN_COLLIDER_DIM: UVec3 = UVec3::new(32, 32, 32);
const WATER_TERRAIN_SINGLE_CHUNK_ID: IVec3 = IVec3::new(1, 0, 1);
const WATER_TERRAIN_COLLIDER_SOURCE: &str = "cpu-solid-voxel-atlas";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WaterTerrainColliderSourceRevision {
    dependencies: Vec<(UVec3, u64)>,
}

pub(super) struct WaterTerrainColliderWorkerJob {
    chunk_key: UVec3,
    chunk_id: IVec3,
    revision: u64,
    source_revision: WaterTerrainColliderSourceRevision,
    solid_chunk: std::sync::Arc<CpuSolidVoxelChunk>,
}

pub(super) struct WaterTerrainColliderWorkerResult {
    chunk_key: UVec3,
    chunk_id: IVec3,
    revision: u64,
    source_revision: WaterTerrainColliderSourceRevision,
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
        self.mark_water_terrain_source_chunk_dirty(chunk_id);
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
        match self.refresh_cpu_solid_voxel_chunk(chunk_id) {
            Ok(Some(revision)) => {
                log::debug!(
                    "[QUEUE][WATER_TERRAIN] CPU solid source ready chunk {:?} source_rev={}",
                    chunk_id,
                    revision,
                );
                self.enqueue_deferred_water_terrain_collider_rebuild(chunk_id);
            }
            Ok(None) => {
                log::debug!(
                    "[QUEUE][WATER_TERRAIN] skipped CPU solid source refresh for invalid chunk {:?}",
                    chunk_id,
                );
            }
            Err(err) => {
                log::error!(
                    "[WATER][TERRAIN] failed to refresh CPU solid source chunk {:?}: {}",
                    chunk_id,
                    err,
                );
            }
        }
    }

    pub(super) fn process_water_terrain_source_updates(&mut self) {
        // Water colliders now consume the CPU solid voxel mirror directly. Drain
        // contree source notifications here so the surface-ray cache does not
        // accumulate stale update records, but do not use them for water invalidation.
        let _ = self.contree_builder.take_cpu_chunk_source_updates();
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

            let build = build_water_terrain_collider_chunk(
                job.solid_chunk.as_ref(),
                job.chunk_id,
                job.revision,
            );
            let result = WaterTerrainColliderWorkerResult {
                chunk_key: job.chunk_key,
                chunk_id: job.chunk_id,
                revision: job.revision,
                source_revision: job.source_revision,
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

        let Some(solid_chunk) = self.cpu_solid_voxels.get(chunk_key) else {
            log::warn!(
                "[WATER][TERRAIN] skipped collider chunk {:?} rev {}: missing CPU solid voxel source",
                chunk_id,
                revision,
            );
            self.deferred_water_terrain_collider_rebuilds
                .complete(chunk_key, revision);
            return;
        };

        let source_revision =
            water_terrain_collider_source_revision(&self.cpu_solid_voxels, chunk_key);
        if self.water_terrain_built_source_revisions.get(&chunk_key) == Some(&source_revision) {
            log::debug!(
                "[QUEUE][WATER_TERRAIN] skip unchanged CPU solid source chunk {:?} revision {} source={:?}",
                chunk_id,
                revision,
                source_revision,
            );
            self.deferred_water_terrain_collider_rebuilds
                .complete(chunk_key, revision);
            return;
        }

        let job = WaterTerrainColliderWorkerJob {
            chunk_key,
            chunk_id,
            revision,
            source_revision,
            solid_chunk,
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
                    "[WATER][TERRAIN] built collider chunk {:?} rev {} latest={} dim {:?} bounds {:?}..{:?} sdf {:.3}..{:.3} sdf_hash {:016x} solid_samples {}/{} source {} source_chunk {:?} source_dim {:?} source_solid_voxels {} center_sdf={:?} build_ms={:.2} phases solid={:.2} count={:.2} sdf={:.2} stats={:.2} queue_pending={}",
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
                    stats.solid.source_chunk,
                    stats.solid.source_dim,
                    stats.solid.source_solid_voxels,
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

            self.water_terrain_built_source_revisions
                .insert(result.chunk_key, result.source_revision);
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

    fn refresh_cpu_solid_voxel_chunk(&mut self, chunk_id: UVec3) -> anyhow::Result<Option<u64>> {
        if chunk_id.cmpge(super::CHUNK_DIM).any() {
            return Ok(None);
        }

        let atlas_offset = chunk_id * VOXEL_DIM_PER_CHUNK;
        let voxel_types = self
            .plain_builder
            .read_chunk_atlas_region(atlas_offset, VOXEL_DIM_PER_CHUNK)?;
        let chunk = self.cpu_solid_voxels.upsert_from_voxel_types(
            chunk_id,
            VOXEL_DIM_PER_CHUNK,
            &voxel_types,
        )?;
        log::debug!(
            "[WATER][TERRAIN] refreshed CPU solid chunk {:?} rev {} solid_voxels {}/{}",
            chunk_id,
            chunk.revision(),
            chunk.solid_count(),
            VOXEL_DIM_PER_CHUNK.x as usize
                * VOXEL_DIM_PER_CHUNK.y as usize
                * VOXEL_DIM_PER_CHUNK.z as usize,
        );
        Ok(Some(chunk.revision()))
    }
}

fn build_water_terrain_collider_chunk(
    solid_chunk: &CpuSolidVoxelChunk,
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
        solid_chunk,
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
    solid_chunk: &CpuSolidVoxelChunk,
    dim: UVec3,
    bounds_min_ws: Vec3,
    bounds_max_ws: Vec3,
) -> (Vec<bool>, WaterTerrainSolidSampleStats) {
    let mut solid = vec![false; grid_len(dim)];
    let stats = WaterTerrainSolidSampleStats {
        source_chunk: solid_chunk.chunk_id(),
        source_dim: solid_chunk.dim(),
        source_solid_voxels: solid_chunk.solid_count(),
    };

    for z in 0..dim.z {
        for y in 0..dim.y {
            for x in 0..dim.x {
                let point_ws = grid_sample_position(bounds_min_ws, bounds_max_ws, dim, x, y, z);
                solid[grid_index(dim, x, y, z)] = solid_chunk.sample_solid_ws(point_ws);
            }
        }
    }

    (solid, stats)
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

#[derive(Clone, Copy, Debug)]
struct WaterTerrainSolidSampleStats {
    source_chunk: UVec3,
    source_dim: UVec3,
    source_solid_voxels: usize,
}

fn water_terrain_chunk_bounds_ws(chunk_id: IVec3) -> (Vec3, Vec3) {
    let min_ws = chunk_id.as_vec3();
    (min_ws, min_ws + Vec3::ONE)
}

fn water_terrain_collider_source_revision(
    cpu_solid_voxels: &crate::app::cpu_solid_voxels::CpuSolidVoxelStore,
    chunk_key: UVec3,
) -> WaterTerrainColliderSourceRevision {
    WaterTerrainColliderSourceRevision {
        dependencies: vec![(chunk_key, cpu_solid_voxels.revision(chunk_key).unwrap_or(0))],
    }
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
    fn source_revision_depends_only_on_same_chunk() {
        let mut store = crate::app::cpu_solid_voxels::CpuSolidVoxelStore::default();
        store
            .upsert_from_voxel_types(UVec3::new(1, 0, 1), UVec3::ONE, &[7])
            .unwrap();

        assert_eq!(
            water_terrain_collider_source_revision(&store, UVec3::new(1, 0, 1)),
            WaterTerrainColliderSourceRevision {
                dependencies: vec![(UVec3::new(1, 0, 1), 1)]
            }
        );
        assert_eq!(
            water_terrain_collider_source_revision(&store, UVec3::new(1, 1, 1)),
            WaterTerrainColliderSourceRevision {
                dependencies: vec![(UVec3::new(1, 1, 1), 0)]
            }
        );
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
