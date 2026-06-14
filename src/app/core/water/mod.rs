use super::{
    App, TerrainSdfColliderRebuildRequest, WaterTerrainCacheRebuildRequest, CHUNK_DIM,
    VOXEL_DIM_PER_CHUNK,
};
use crate::app::cpu_solid_voxels::CpuSolidVoxelChunk;
use crate::app::world_edits::TerrainRemovalEdit;
use crate::app::GuiAdjustables;
use crate::builder::{ChunkSolidSampleJob, ChunkSolidSampleResult, VOXEL_TYPE_ROCK};
use glam::{IVec3, UVec3, Vec2, Vec3};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use verdarium_terrain_collider::signed_distance_from_solid_samples;
use verdarium_water::{
    build_terrain_grid_cache_patch, PondWaterConfig, WaterTerrainCacheBuildRequest,
    WaterTerrainCachePatch, WaterTerrainColliderChunk,
};

const TERRAIN_SDF_COLLIDER_DIM: UVec3 = UVec3::new(32, 32, 32);
const TERRAIN_SDF_COLLIDER_SOURCE: &str = "gpu-sampled-solid-grid";
const TERRAIN_SDF_SOURCE_REFRESH_COALESCE_DELAY: Duration = Duration::from_millis(150);
const TERRAIN_SDF_SOURCE_LOW_PRIORITY_DELAY: Duration = Duration::from_secs(2);
const WATER_TERRAIN_ACTIVE_PARTICLE_HALO_CELLS: f32 = 8.0;
const WATER_TERRAIN_ACTIVE_MAX_SUBSTEPS: usize = 2;
mod runtime;
pub(super) use runtime::AsyncWaterSim;

pub(super) fn apply_water_gui_adjustables_to_config(
    config: &mut PondWaterConfig,
    gui_adjustables: &GuiAdjustables,
) {
    let substep_hz = finite_at_least(
        gui_adjustables.water_substep_hz.value,
        1.0,
        config.substep_dt.recip(),
    );
    config.substep_dt = substep_hz.recip();
    let particle_edge_len = finite_at_least(
        gui_adjustables.water_particle_edge_len.value,
        1.0e-6,
        config.particle_volume.cbrt(),
    );
    config.set_particle_edge_len(particle_edge_len);
    config.terrain_collision_margin_cells = finite_at_least(
        gui_adjustables.water_terrain_margin_cells.value,
        0.0,
        config.terrain_collision_margin_cells,
    );
    config.terrain_density_min_fluid_fraction = finite_clamped(
        gui_adjustables
            .water_boundary_density_min_fluid_fraction
            .value,
        1.0e-3,
        1.0,
        config.terrain_density_min_fluid_fraction,
    );
    config.terrain_density_max_correction_factor = finite_at_least(
        gui_adjustables
            .water_boundary_density_max_correction_factor
            .value,
        1.0,
        config.terrain_density_max_correction_factor,
    );
    config.terrain_density_occupancy_transition_cells = finite_at_least(
        gui_adjustables
            .water_boundary_density_occupancy_transition_cells
            .value,
        1.0e-3,
        config.terrain_density_occupancy_transition_cells,
    );
    config.linear_damping_per_sec = finite_at_least(
        gui_adjustables.water_damping.value,
        0.0,
        config.linear_damping_per_sec,
    );
    config.quiet_settling_velocity_damping_per_sec = finite_at_least(
        gui_adjustables.water_quiet_settling_velocity_damping.value,
        0.0,
        config.quiet_settling_velocity_damping_per_sec,
    );
    config.quiet_settling_affine_damping_per_sec = finite_at_least(
        gui_adjustables.water_quiet_settling_affine_damping.value,
        0.0,
        config.quiet_settling_affine_damping_per_sec,
    );
    config.debug_spawn_height_offset = finite_at_least(
        gui_adjustables.water_debug_spawn_height_offset.value,
        0.0,
        config.debug_spawn_height_offset,
    );
    config.terrain_tangent_damping_per_sec = finite_at_least(
        gui_adjustables.water_terrain_tangent_damping.value,
        0.0,
        config.terrain_tangent_damping_per_sec,
    );
    config.gravity.y = finite_or(gui_adjustables.water_gravity_y.value, config.gravity.y);
    config.stiffness =
        finite_at_least(gui_adjustables.water_stiffness.value, 0.0, config.stiffness);
    config.gamma = finite_at_least(gui_adjustables.water_gamma.value, 0.0, config.gamma);
    config.j_min = finite_clamped(gui_adjustables.water_j_min.value, 1.0e-4, 1.0, config.j_min);
    config.wall_damping = finite_clamped(
        gui_adjustables.water_wall_damping.value,
        0.0,
        1.0,
        config.wall_damping,
    );
}

pub(super) fn sync_water_gui_adjustables_from_config(
    gui_adjustables: &mut GuiAdjustables,
    config: &PondWaterConfig,
) {
    gui_adjustables.water_substep_hz.value = config.substep_dt.recip();
    gui_adjustables.water_particle_edge_len.value = config.particle_volume.cbrt();
    gui_adjustables.water_terrain_margin_cells.value = config.terrain_collision_margin_cells;
    gui_adjustables
        .water_boundary_density_min_fluid_fraction
        .value = config.terrain_density_min_fluid_fraction;
    gui_adjustables
        .water_boundary_density_max_correction_factor
        .value = config.terrain_density_max_correction_factor;
    gui_adjustables
        .water_boundary_density_occupancy_transition_cells
        .value = config.terrain_density_occupancy_transition_cells;
    gui_adjustables.water_damping.value = config.linear_damping_per_sec;
    gui_adjustables.water_quiet_settling_velocity_damping.value =
        config.quiet_settling_velocity_damping_per_sec;
    gui_adjustables.water_quiet_settling_affine_damping.value =
        config.quiet_settling_affine_damping_per_sec;
    gui_adjustables.water_debug_spawn_height_offset.value = config.debug_spawn_height_offset;
    gui_adjustables.water_terrain_tangent_damping.value = config.terrain_tangent_damping_per_sec;
    gui_adjustables.water_gravity_y.value = config.gravity.y;
    gui_adjustables.water_stiffness.value = config.stiffness;
    gui_adjustables.water_gamma.value = config.gamma;
    gui_adjustables.water_j_min.value = config.j_min;
    gui_adjustables.water_wall_damping.value = config.wall_damping;
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn finite_at_least(value: f32, min: f32, fallback: f32) -> f32 {
    finite_or(value, fallback).max(min)
}

fn finite_clamped(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    finite_or(value, fallback).clamp(min, max)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TerrainSdfSourceRevision {
    dependencies: Vec<(UVec3, u64)>,
}

pub(super) struct TerrainSdfSourceRefreshInFlight {
    chunk_id: UVec3,
    revision: u64,
    submitted_at: Instant,
    job: ChunkSolidSampleJob,
}

pub(super) struct TerrainSdfColliderWorkerJob {
    chunk_key: UVec3,
    chunk_id: IVec3,
    revision: u64,
    source_revision: TerrainSdfSourceRevision,
    solid_chunk: std::sync::Arc<CpuSolidVoxelChunk>,
}

pub(super) struct TerrainSdfColliderWorkerResult {
    chunk_key: UVec3,
    chunk_id: IVec3,
    revision: u64,
    source_revision: TerrainSdfSourceRevision,
    build: Option<TerrainSdfColliderBuild>,
}

pub(super) struct WaterTerrainCacheWorkerJob {
    chunk_key: UVec3,
    chunk_id: IVec3,
    revision: u64,
    request: WaterTerrainCacheBuildRequest,
}

pub(super) struct WaterTerrainCacheWorkerResult {
    chunk_key: UVec3,
    chunk_id: IVec3,
    revision: u64,
    patch: WaterTerrainCachePatch,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TerrainSdfSourceRefreshRequest {
    ready_at: Instant,
}

#[derive(Clone, Debug, Default)]
pub(super) struct WaterEditSoak {
    next_step: usize,
}

#[derive(Clone, Copy, Debug)]
enum WaterEditSoakOp {
    Remove,
    PlaceRock,
}

#[derive(Clone, Copy, Debug)]
struct WaterEditSoakStep {
    delay_sec: f32,
    label: &'static str,
    xz: Vec2,
    radius: f32,
    op: WaterEditSoakOp,
}

impl App {
    pub(super) fn enqueue_startup_water_terrain_collider_rebuilds(&mut self) {
        let mut enqueued = 0usize;
        let mut skipped = 0usize;
        let bounds = self.water_sim.config.collider;
        for x in 0..CHUNK_DIM.x {
            for y in 0..CHUNK_DIM.y {
                for z in 0..CHUNK_DIM.z {
                    let chunk_id = UVec3::new(x, y, z);
                    if !water_terrain_chunk_key_intersects_box_grid_domain(
                        chunk_id,
                        bounds.min_ws,
                        bounds.max_ws,
                    ) {
                        skipped += 1;
                        continue;
                    }
                    self.schedule_terrain_sdf_source_refresh_immediate(chunk_id);
                    enqueued += 1;
                }
            }
        }
        log::info!(
            "[WATER][TERRAIN] enqueued startup collider rebuilds for {} water-domain chunks, skipped {} out-of-domain chunks",
            enqueued,
            skipped,
        );
    }

    pub(super) fn enqueue_deferred_terrain_sdf_collider_rebuild(&mut self, chunk_id: UVec3) {
        let revision = self
            .deferred_terrain_sdf_collider_rebuilds
            .push(chunk_id, TerrainSdfColliderRebuildRequest);
        log::debug!(
            "[QUEUE][TERRAIN_SDF_COLLIDER] enqueue chunk {:?} revision {} pending={} active={}",
            chunk_id,
            revision,
            self.deferred_terrain_sdf_collider_rebuilds.len(),
            self.deferred_terrain_sdf_collider_rebuilds.active_len(),
        );
    }

    fn enqueue_deferred_water_terrain_cache_rebuild(&mut self, chunk_id: IVec3) {
        let Some(chunk_key) = water_terrain_chunk_id_to_uvec3(chunk_id) else {
            log::warn!(
                "[WATER][TERRAIN_CACHE] skipped invalid cache rebuild chunk {:?}",
                chunk_id,
            );
            return;
        };
        let revision = self
            .deferred_water_terrain_cache_rebuilds
            .push(chunk_key, WaterTerrainCacheRebuildRequest);
        log::debug!(
            "[QUEUE][WATER_TERRAIN_CACHE] enqueue chunk {:?} revision {} pending={} active={}",
            chunk_id,
            revision,
            self.deferred_water_terrain_cache_rebuilds.len(),
            self.deferred_water_terrain_cache_rebuilds.active_len(),
        );
    }

    fn enqueue_deferred_terrain_sdf_source_refresh(&mut self, chunk_id: UVec3, delay: Duration) {
        let now = Instant::now();
        let request = TerrainSdfSourceRefreshRequest {
            ready_at: now + delay,
        };
        // Repeated dirty notifications for the same chunk should keep the
        // latest revision, but must not slide ready_at forever while the user
        // holds a terrain-edit tool. Preserve the earliest ready time so the
        // budgeted queue can keep publishing intermediate collider updates.
        let revision = self.deferred_terrain_sdf_source_refreshes.push_coalesced(
            chunk_id,
            request,
            |existing, pending| existing.coalesce_keep_earliest_ready(pending),
        );
        log::debug!(
            "[QUEUE][TERRAIN_SDF_SOURCE] enqueue chunk {:?} revision {} coalesce_delay_ms={:.1} pending={} active={}",
            chunk_id,
            revision,
            delay.as_secs_f32() * 1000.0,
            self.deferred_terrain_sdf_source_refreshes.len(),
            self.deferred_terrain_sdf_source_refreshes.active_len(),
        );
    }

    fn schedule_terrain_sdf_source_refresh_immediate(&mut self, chunk_id: UVec3) {
        self.schedule_terrain_sdf_source_refresh_after(chunk_id, Duration::ZERO);
    }

    pub(super) fn schedule_terrain_sdf_source_refresh(&mut self, chunk_id: UVec3) {
        self.schedule_terrain_sdf_source_refresh_after(
            chunk_id,
            TERRAIN_SDF_SOURCE_REFRESH_COALESCE_DELAY,
        );
    }

    fn schedule_terrain_sdf_source_refresh_after(&mut self, chunk_id: UVec3, delay: Duration) {
        let bounds = self.water_sim.config.collider;
        if !water_terrain_chunk_key_intersects_box_grid_domain(
            chunk_id,
            bounds.min_ws,
            bounds.max_ws,
        ) {
            log::debug!(
                "[WATER][TERRAIN] skipped collider source refresh for out-of-water-domain chunk {:?}",
                chunk_id,
            );
            return;
        }

        let delay = self.terrain_sdf_source_refresh_delay_for_activity(chunk_id, delay);
        self.enqueue_deferred_terrain_sdf_source_refresh(chunk_id, delay);
    }

    fn terrain_sdf_source_refresh_delay_for_activity(
        &self,
        chunk_id: UVec3,
        base_delay: Duration,
    ) -> Duration {
        if base_delay.is_zero() {
            return base_delay;
        }

        let Some((particle_min_ws, particle_max_ws)) = self.water_sim.particle_bounds_ws() else {
            log::debug!(
                "[WATER][TERRAIN] low-priority collider source refresh for chunk {:?}: no active water particles",
                chunk_id,
            );
            return base_delay.max(TERRAIN_SDF_SOURCE_LOW_PRIORITY_DELAY);
        };

        let halo_ws = (self.water_sim.dx * WATER_TERRAIN_ACTIVE_PARTICLE_HALO_CELLS).max(0.0);
        let active_min_ws = particle_min_ws - Vec3::splat(halo_ws);
        let active_max_ws = particle_max_ws + Vec3::splat(halo_ws);
        if water_terrain_chunk_key_intersects_box_grid_domain(
            chunk_id,
            active_min_ws,
            active_max_ws,
        ) {
            return base_delay;
        }

        log::debug!(
            "[WATER][TERRAIN] low-priority collider source refresh for chunk {:?}: outside active water bounds {:?}..{:?} halo={:.3}",
            chunk_id,
            particle_min_ws,
            particle_max_ws,
            halo_ws,
        );
        base_delay.max(TERRAIN_SDF_SOURCE_LOW_PRIORITY_DELAY)
    }

    pub(super) fn process_terrain_sdf_source_updates(&mut self) {
        // Water collider invalidation is driven by edit bounds. The collider
        // source itself is a low-res CPU solid grid sampled directly from the
        // GPU atlas, so drain contree notifications here only to keep the
        // surface-ray cache from accumulating stale update records.
        let _ = self.contree_builder.take_cpu_chunk_source_updates();
        self.process_deferred_terrain_sdf_source_refreshes();
    }

    fn process_deferred_terrain_sdf_source_refreshes(&mut self) {
        self.publish_completed_terrain_sdf_source_refresh();
        self.try_submit_next_terrain_sdf_source_refresh();
    }

    fn try_submit_next_terrain_sdf_source_refresh(&mut self) {
        if self.terrain_sdf_source_refresh_inflight.is_some() {
            return;
        }

        let focus = self.water_terrain_focus_ws();
        let now = Instant::now();
        let Some(work) = self
            .deferred_terrain_sdf_source_refreshes
            .pop_nearest_to_if_payload(focus, UVec3::ONE, |_, request| request.ready_at <= now)
        else {
            return;
        };

        let chunk_id = work.chunk_id;
        let revision = work.revision;
        match self.submit_terrain_sdf_solid_sample_chunk(chunk_id) {
            Ok(Some(job)) => {
                let sample_count = job.sample_count();
                let byte_count = job.byte_count();
                let atlas_offset = job.atlas_offset();
                let atlas_dim = job.atlas_dim();
                let sample_dim = job.sample_dim();
                self.terrain_sdf_source_refresh_inflight = Some(TerrainSdfSourceRefreshInFlight {
                    chunk_id,
                    revision,
                    submitted_at: Instant::now(),
                    job,
                });
                log::debug!(
                    "[QUEUE][TERRAIN_SDF_SOURCE] submit async chunk {:?} revision {} atlas_offset={:?} source_dim={:?} sample_dim={:?} samples={} readback_bytes={} pending={} active={}",
                    chunk_id,
                    revision,
                    atlas_offset,
                    atlas_dim,
                    sample_dim,
                    sample_count,
                    byte_count,
                    self.deferred_terrain_sdf_source_refreshes.len(),
                    self.deferred_terrain_sdf_source_refreshes.active_len(),
                );
            }
            Ok(None) => {
                log::debug!(
                    "[QUEUE][TERRAIN_SDF_SOURCE] skipped invalid chunk {:?} revision {}",
                    chunk_id,
                    revision,
                );
                self.deferred_terrain_sdf_source_refreshes
                    .complete(chunk_id, revision);
            }
            Err(err) => {
                log::error!(
                    "[WATER][TERRAIN] failed to submit solid source refresh chunk {:?} revision {}: {}",
                    chunk_id,
                    revision,
                    err,
                );
                self.deferred_terrain_sdf_source_refreshes
                    .complete(chunk_id, revision);
            }
        }
    }

    fn publish_completed_terrain_sdf_source_refresh(&mut self) {
        let Some(active) = self.terrain_sdf_source_refresh_inflight.as_ref() else {
            return;
        };
        let ready = match self
            .plain_builder
            .chunk_atlas_solid_grid_sample_ready(&active.job)
        {
            Ok(ready) => ready,
            Err(err) => {
                log::error!(
                    "[WATER][TERRAIN] failed to poll solid source refresh chunk {:?} revision {}: {}",
                    active.chunk_id,
                    active.revision,
                    err,
                );
                return;
            }
        };
        if !ready {
            return;
        }

        let active = self
            .terrain_sdf_source_refresh_inflight
            .take()
            .expect("terrain SDF source refresh disappeared after readiness poll");
        let chunk_id = active.chunk_id;
        let revision = active.revision;
        let age_ms = active.submitted_at.elapsed().as_secs_f64() * 1000.0;
        let is_latest = self
            .deferred_terrain_sdf_source_refreshes
            .is_latest_revision(chunk_id, revision);
        if !is_latest {
            log::debug!(
                "[WATER][TERRAIN] discarded stale solid source refresh chunk {:?} rev {} latest_pending=true age={:.2}ms pending={} active={}",
                chunk_id,
                revision,
                age_ms,
                self.deferred_terrain_sdf_source_refreshes.len(),
                self.deferred_terrain_sdf_source_refreshes.active_len(),
            );
            self.deferred_terrain_sdf_source_refreshes
                .complete(chunk_id, revision);
            return;
        }

        match self.finish_terrain_sdf_solid_sample_chunk(chunk_id, active.job) {
            Ok(source_refresh) => {
                let source_revision = terrain_sdf_source_revision(&self.cpu_solid_voxels, chunk_id);
                let already_built = self.terrain_sdf_built_source_revisions.get(&chunk_id)
                    == Some(&source_revision);
                if source_refresh.changed || !already_built {
                    log::debug!(
                        "[QUEUE][TERRAIN_SDF_SOURCE] ready chunk {:?} revision {} source_rev={} changed={} already_built={}",
                        chunk_id,
                        revision,
                        source_refresh.revision,
                        source_refresh.changed,
                        already_built,
                    );
                    self.enqueue_deferred_terrain_sdf_collider_rebuild(chunk_id);
                } else {
                    log::debug!(
                        "[QUEUE][TERRAIN_SDF_SOURCE] skipped unchanged chunk {:?} revision {} source_rev={} pending={} active={}",
                        chunk_id,
                        revision,
                        source_refresh.revision,
                        self.deferred_terrain_sdf_source_refreshes.len(),
                        self.deferred_terrain_sdf_source_refreshes.active_len(),
                    );
                }
            }
            Err(err) => {
                log::error!(
                    "[WATER][TERRAIN] failed to finish solid source refresh chunk {:?} revision {}: {}",
                    chunk_id,
                    revision,
                    err,
                );
            }
        }
        self.deferred_terrain_sdf_source_refreshes
            .complete(chunk_id, revision);
    }

    pub(super) fn process_deferred_water_terrain_cache_rebuild(&mut self) {
        self.publish_completed_water_terrain_cache_rebuilds();
        self.try_submit_next_water_terrain_cache_rebuild();
    }

    fn try_submit_next_water_terrain_cache_rebuild(&mut self) {
        if self.water_terrain_cache_rebuild_inflight {
            return;
        }

        let focus = self.water_terrain_focus_ws();
        let Some(work) = self
            .deferred_water_terrain_cache_rebuilds
            .pop_nearest_to(focus, UVec3::ONE)
        else {
            return;
        };
        let chunk_key = work.chunk_id;
        let revision = work.revision;
        let Some(chunk_id) = water_terrain_chunk_work_key_to_id(chunk_key) else {
            log::warn!(
                "[WATER][TERRAIN_CACHE] skipped invalid queued cache rebuild chunk {:?} rev {}",
                chunk_key,
                revision,
            );
            self.deferred_water_terrain_cache_rebuilds
                .complete(chunk_key, revision);
            return;
        };

        let Some(request) = self
            .water_sim
            .terrain_grid_cache_build_request_for_chunk(chunk_id)
        else {
            log::info!(
                "[WATER][TERRAIN_CACHE] skipped worker grid cache region chunk={:?} outside=true",
                chunk_id,
            );
            self.deferred_water_terrain_cache_rebuilds
                .complete(chunk_key, revision);
            return;
        };

        let node_count = request.node_count();
        let min_node = request.min_node();
        let max_node_exclusive = request.max_node_exclusive();
        let terrain_chunk_count = request.terrain_chunk_count();
        let grid_dim = request.grid_dim();
        let near_surface_band = request.near_surface_band();
        let dx = request.dx();
        let job = WaterTerrainCacheWorkerJob {
            chunk_key,
            chunk_id,
            revision,
            request,
        };
        self.water_terrain_cache_rebuild_inflight = true;
        if let Err(err) = self.water_terrain_cache_job_tx.send(job) {
            log::error!(
                "[WATER][TERRAIN_CACHE] failed to submit worker cache rebuild chunk {:?} rev {}: {}",
                chunk_id,
                revision,
                err,
            );
            self.water_terrain_cache_rebuild_inflight = false;
            self.deferred_water_terrain_cache_rebuilds
                .complete(chunk_key, revision);
            return;
        }

        log::debug!(
            "[QUEUE][WATER_TERRAIN_CACHE] submit worker chunk {:?} revision {} chunks={} grid {:?} range {:?}..{:?} nodes={} band={:.5} dx={:.5} pending={} active={}",
            chunk_id,
            revision,
            terrain_chunk_count,
            grid_dim,
            min_node,
            max_node_exclusive,
            node_count,
            near_surface_band,
            dx,
            self.deferred_water_terrain_cache_rebuilds.len(),
            self.deferred_water_terrain_cache_rebuilds.active_len(),
        );
    }

    fn publish_completed_water_terrain_cache_rebuilds(&mut self) {
        while let Ok(result) = self.water_terrain_cache_result_rx.try_recv() {
            self.water_terrain_cache_rebuild_inflight = false;
            let is_latest = self
                .deferred_water_terrain_cache_rebuilds
                .is_latest_revision(result.chunk_key, result.revision);

            if !is_latest {
                log::debug!(
                    "[WATER][TERRAIN_CACHE] discarded stale worker grid cache region chunk {:?} rev {} latest_pending=true worker_ms={:.2}",
                    result.chunk_id,
                    result.revision,
                    result.patch.build_ms(),
                );
                self.deferred_water_terrain_cache_rebuilds
                    .complete(result.chunk_key, result.revision);
                continue;
            }

            self.water_sim.submit_terrain_grid_cache_patch(result.patch);

            self.deferred_water_terrain_cache_rebuilds
                .complete(result.chunk_key, result.revision);
        }
    }

    pub(super) fn process_deferred_terrain_sdf_collider_rebuild(&mut self) {
        self.publish_completed_terrain_sdf_collider_rebuilds();
        self.try_submit_next_terrain_sdf_collider_rebuild();
    }

    pub(super) fn update_water_sim(&mut self, frame_delta_time: f32, world_tick_seconds: f32) {
        self.water_sim.apply_gui_adjustables(&self.gui_adjustables);
        let max_substeps = if self.water_terrain_work_active() {
            WATER_TERRAIN_ACTIVE_MAX_SUBSTEPS
        } else {
            runtime::WATER_SIM_THREAD_DEFAULT_MAX_SUBSTEPS
        };
        let water_world_tick_multiplier = self
            .gui_adjustables
            .water_world_tick_multiplier
            .value
            .clamp(0.0, 1.0);
        let water_tick_seconds = crate::game_time::clamp_world_tick_seconds(
            world_tick_seconds * water_world_tick_multiplier,
        );
        let snapshot_interval = Duration::from_secs_f32(water_tick_seconds);
        self.water_sim.set_runtime_options(
            true,
            self.perf_logging,
            max_substeps,
            snapshot_interval,
        );

        self.water_sim
            .poll_latest_snapshot_after_frame(frame_delta_time, water_tick_seconds);
    }

    fn water_terrain_work_active(&self) -> bool {
        !self.deferred_chunk_rebuilds_idle()
            || !self.deferred_terrain_sdf_source_refreshes.is_idle()
            || !self.deferred_terrain_sdf_collider_rebuilds.is_idle()
            || !self.deferred_water_terrain_cache_rebuilds.is_idle()
    }

    pub(super) fn process_water_edit_soak(&mut self) {
        let Some(render_start) = self.render_start_time else {
            return;
        };
        let Some(current_step) = self.water_edit_soak.as_ref().map(|soak| soak.next_step) else {
            return;
        };
        let Some(step) = water_edit_soak_step(current_step) else {
            return;
        };

        if render_start.elapsed().as_secs_f32() < step.delay_sec {
            return;
        }
        if !self.water_terrain_initialized
            || !self.deferred_chunk_rebuilds_idle()
            || !self.deferred_terrain_sdf_source_refreshes.is_idle()
            || !self.deferred_terrain_sdf_collider_rebuilds.is_idle()
            || !self.deferred_water_terrain_cache_rebuilds.is_idle()
        {
            return;
        }

        let next_step = current_step + 1;
        if let Err(err) = self.apply_water_edit_soak_step(step) {
            log::error!(
                "[WATER][EDIT_SOAK] step {} ({}) failed: {}",
                current_step,
                step.label,
                err,
            );
        }
        if let Some(soak) = &mut self.water_edit_soak {
            soak.next_step = next_step;
            if water_edit_soak_step(soak.next_step).is_none() {
                log::info!("[WATER][EDIT_SOAK] completed deterministic terrain-edit sequence");
            }
        }
    }

    fn apply_water_edit_soak_step(&mut self, step: WaterEditSoakStep) -> anyhow::Result<()> {
        let terrain_height = self.query_terrain_height_cpu(step.xz);
        let center = Vec3::new(step.xz.x, terrain_height, step.xz.y);
        match step.op {
            WaterEditSoakOp::Remove => {
                let readback = self.apply_surface_terrain_removal(
                    TerrainRemovalEdit {
                        center,
                        radius: step.radius,
                    },
                    None,
                    Some(65_536),
                    None,
                )?;
                let removed: u32 = readback.stats.removed_counts.iter().sum();
                log::info!(
                    "[WATER][EDIT_SOAK] applied {} center=({:.3},{:.3},{:.3}) radius={:.3} removed_voxels={} sampled_positions={}",
                    step.label,
                    center.x,
                    center.y,
                    center.z,
                    step.radius,
                    removed,
                    readback.sampled_positions_world.len(),
                );
            }
            WaterEditSoakOp::PlaceRock => {
                let readback = self.apply_surface_terrain_placement(
                    TerrainRemovalEdit {
                        center,
                        radius: step.radius,
                    },
                    VOXEL_TYPE_ROCK,
                    65_536,
                )?;
                let added: u32 = readback.stats.added_counts.iter().sum();
                log::info!(
                    "[WATER][EDIT_SOAK] applied {} center=({:.3},{:.3},{:.3}) radius={:.3} added_voxels={} sampled_positions={}",
                    step.label,
                    center.x,
                    center.y,
                    center.z,
                    step.radius,
                    added,
                    readback.sampled_positions_world.len(),
                );
            }
        }
        Ok(())
    }

    pub(super) fn spawn_terrain_sdf_collider_worker() -> (
        mpsc::Sender<TerrainSdfColliderWorkerJob>,
        mpsc::Receiver<TerrainSdfColliderWorkerResult>,
    ) {
        let (job_tx, job_rx) = mpsc::channel::<TerrainSdfColliderWorkerJob>();
        let (result_tx, result_rx) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                let build = build_terrain_sdf_collider_chunk(
                    job.solid_chunk.as_ref(),
                    job.chunk_id,
                    job.revision,
                );
                let result = TerrainSdfColliderWorkerResult {
                    chunk_key: job.chunk_key,
                    chunk_id: job.chunk_id,
                    revision: job.revision,
                    source_revision: job.source_revision,
                    build,
                };
                if result_tx.send(result).is_err() {
                    break;
                }
            }
        });

        (job_tx, result_rx)
    }

    pub(super) fn spawn_water_terrain_cache_worker() -> (
        mpsc::Sender<WaterTerrainCacheWorkerJob>,
        mpsc::Receiver<WaterTerrainCacheWorkerResult>,
    ) {
        let (job_tx, job_rx) = mpsc::channel::<WaterTerrainCacheWorkerJob>();
        let (result_tx, result_rx) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                let patch = build_terrain_grid_cache_patch(job.request);
                let result = WaterTerrainCacheWorkerResult {
                    chunk_key: job.chunk_key,
                    chunk_id: job.chunk_id,
                    revision: job.revision,
                    patch,
                };
                if result_tx.send(result).is_err() {
                    break;
                }
            }
        });

        (job_tx, result_rx)
    }

    fn try_submit_next_terrain_sdf_collider_rebuild(&mut self) {
        if self.terrain_sdf_collider_build_inflight {
            return;
        }

        let focus = self.water_terrain_focus_ws();
        let Some(work) = self
            .deferred_terrain_sdf_collider_rebuilds
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
            self.deferred_terrain_sdf_collider_rebuilds
                .complete(chunk_key, revision);
            return;
        };

        let Some(solid_chunk) = self.cpu_solid_voxels.get(chunk_key) else {
            log::warn!(
                "[WATER][TERRAIN] skipped collider chunk {:?} rev {}: missing solid source",
                chunk_id,
                revision,
            );
            self.deferred_terrain_sdf_collider_rebuilds
                .complete(chunk_key, revision);
            return;
        };

        let source_revision = terrain_sdf_source_revision(&self.cpu_solid_voxels, chunk_key);
        if self.terrain_sdf_built_source_revisions.get(&chunk_key) == Some(&source_revision) {
            log::debug!(
                "[QUEUE][TERRAIN_SDF_COLLIDER] skip unchanged solid source chunk {:?} revision {} source={:?}",
                chunk_id,
                revision,
                source_revision,
            );
            self.deferred_terrain_sdf_collider_rebuilds
                .complete(chunk_key, revision);
            return;
        }

        if solid_chunk.solid_count() == 0 {
            let removed = self.remove_empty_water_terrain_collider_chunk(chunk_id);
            log::debug!(
                "[WATER][TERRAIN] skipped collider chunk {:?} rev {}: empty solid source chunk {:?} removed_stale_collider={}",
                chunk_id,
                revision,
                chunk_key,
                removed,
            );
            self.terrain_sdf_built_source_revisions
                .insert(chunk_key, source_revision);
            self.deferred_terrain_sdf_collider_rebuilds
                .complete(chunk_key, revision);
            self.finish_startup_water_terrain_collider_batch_if_ready();
            return;
        }

        let job = TerrainSdfColliderWorkerJob {
            chunk_key,
            chunk_id,
            revision,
            source_revision,
            solid_chunk,
        };
        self.terrain_sdf_collider_build_inflight = true;
        if let Err(err) = self.terrain_sdf_collider_job_tx.send(job) {
            log::error!(
                "[WATER][TERRAIN] failed to submit collider build chunk {:?} rev {}: {}",
                chunk_id,
                revision,
                err,
            );
            self.terrain_sdf_collider_build_inflight = false;
            self.deferred_terrain_sdf_collider_rebuilds
                .complete(chunk_key, revision);
            return;
        }

        log::debug!(
            "[QUEUE][TERRAIN_SDF_COLLIDER] submit chunk {:?} revision {} pending={} active={}",
            chunk_id,
            revision,
            self.deferred_terrain_sdf_collider_rebuilds.len(),
            self.deferred_terrain_sdf_collider_rebuilds.active_len(),
        );
    }

    fn publish_completed_terrain_sdf_collider_rebuilds(&mut self) {
        while let Ok(result) = self.terrain_sdf_collider_result_rx.try_recv() {
            self.terrain_sdf_collider_build_inflight = false;
            let is_latest = self
                .deferred_terrain_sdf_collider_rebuilds
                .is_latest_revision(result.chunk_key, result.revision);

            if !is_latest {
                log::debug!(
                    "[WATER][TERRAIN] discarded stale collider chunk {:?} rev {} latest_pending=true",
                    result.chunk_id,
                    result.revision,
                );
                self.deferred_terrain_sdf_collider_rebuilds
                    .complete(result.chunk_key, result.revision);
                continue;
            }

            if let Some(build) = result.build {
                let TerrainSdfColliderBuild {
                    chunk,
                    bounds_min_ws,
                    bounds_max_ws,
                    stats,
                } = build;
                let center_probe = (bounds_min_ws + bounds_max_ws) * 0.5;
                if self.water_terrain_initialized {
                    self.publish_water_terrain_collider_chunk(chunk);
                } else {
                    self.publish_startup_water_terrain_collider_chunk_deferred(chunk);
                }
                let center_sdf = self
                    .water_sim
                    .terrain_collider_set()
                    .and_then(|set| set.sample_sdf_ws(center_probe));
                log::info!(
                    "[WATER][TERRAIN] built collider chunk {:?} rev {} latest={} dim {:?} bounds {:?}..{:?} sdf {:.3}..{:.3} sdf_hash {:016x} solid_samples {}/{} source {} source_chunk {:?} source_dim {:?} source_solid_voxels {} center_sdf={:?} build_ms={:.2} phases solid={:.2} count={:.2} sdf={:.2} stats={:.2} queue_pending={}",
                    result.chunk_id,
                    result.revision,
                    is_latest,
                    TERRAIN_SDF_COLLIDER_DIM,
                    bounds_min_ws,
                    bounds_max_ws,
                    stats.min_sdf,
                    stats.max_sdf,
                    stats.sdf_hash,
                    stats.solid_sample_count,
                    stats.sample_count,
                    TERRAIN_SDF_COLLIDER_SOURCE,
                    stats.solid.source_chunk,
                    stats.solid.source_dim,
                    stats.solid.source_solid_voxels,
                    center_sdf,
                    stats.build_ms,
                    stats.solid_ms,
                    stats.count_ms,
                    stats.sdf_ms,
                    stats.stats_ms,
                    self.deferred_terrain_sdf_collider_rebuilds.len(),
                );
            }

            self.terrain_sdf_built_source_revisions
                .insert(result.chunk_key, result.source_revision);
            self.deferred_terrain_sdf_collider_rebuilds
                .complete(result.chunk_key, result.revision);
            self.finish_startup_water_terrain_collider_batch_if_ready();
        }
    }

    fn remove_empty_water_terrain_collider_chunk(&mut self, chunk_id: IVec3) -> bool {
        let removed = self
            .water_sim
            .remove_terrain_collider_chunk_deferred(chunk_id);
        if removed {
            if self.water_terrain_initialized {
                self.water_sim
                    .invalidate_terrain_grid_cache_for_chunk(chunk_id);
                self.enqueue_deferred_water_terrain_cache_rebuild(chunk_id);
            } else {
                self.water_terrain_collider_cache_rebuild_pending = true;
            }
        }
        removed
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
        self.water_sim.upsert_terrain_collider_chunk_deferred(chunk);
        self.water_sim
            .invalidate_terrain_grid_cache_for_chunk(chunk_id);
        self.enqueue_deferred_water_terrain_cache_rebuild(chunk_id);
        if should_stabilize_particles {
            self.water_sim
                .stabilize_after_terrain_chunk_change(chunk_id);
        }
    }

    fn publish_startup_water_terrain_collider_chunk_deferred(
        &mut self,
        chunk: WaterTerrainColliderChunk,
    ) {
        let chunk_id = chunk.chunk_id;
        self.water_sim.upsert_terrain_collider_chunk_deferred(chunk);
        if water_terrain_chunk_strictly_overlaps_box(
            chunk_id,
            self.water_sim.config.collider.min_ws,
            self.water_sim.config.collider.max_ws,
        ) {
            self.water_terrain_collider_cache_rebuild_pending = true;
        }
    }

    fn finish_startup_water_terrain_collider_batch_if_ready(&mut self) {
        if self.water_terrain_initialized || !self.water_terrain_has_startup_collider() {
            return;
        }

        if self.water_terrain_collider_cache_rebuild_pending {
            self.water_sim.finish_terrain_collider_chunk_batch(true);
            self.water_terrain_collider_cache_rebuild_pending = false;
        }
        self.water_terrain_initialized = true;
        log::info!("[WATER][TERRAIN] initialized startup collider batch");
    }

    fn water_terrain_focus_ws(&self) -> Vec3 {
        // Prefer camera position so nearby chunks are built first,
        // especially after the collider covers the full world.
        let camera = self.tracer.camera_position();
        let bounds = self.water_sim.config.collider;
        camera.clamp(bounds.min_ws, bounds.max_ws)
    }

    fn water_terrain_has_startup_collider(&self) -> bool {
        if self.terrain_sdf_collider_build_inflight
            || !self.deferred_terrain_sdf_source_refreshes.is_idle()
            || !self.deferred_terrain_sdf_collider_rebuilds.is_idle()
            || !self.deferred_water_terrain_cache_rebuilds.is_idle()
        {
            return false;
        }

        let bounds = self.water_sim.config.collider;
        self.water_sim.terrain_collider_set().is_some_and(|set| {
            set.chunks.keys().any(|&chunk_id| {
                water_terrain_chunk_strictly_overlaps_box(chunk_id, bounds.min_ws, bounds.max_ws)
            })
        })
    }

    fn submit_terrain_sdf_solid_sample_chunk(
        &mut self,
        chunk_id: UVec3,
    ) -> anyhow::Result<Option<ChunkSolidSampleJob>> {
        if chunk_id.cmpge(super::CHUNK_DIM).any() {
            return Ok(None);
        }

        let atlas_offset = chunk_id * VOXEL_DIM_PER_CHUNK;
        self.plain_builder
            .submit_chunk_atlas_solid_grid_sample(
                atlas_offset,
                VOXEL_DIM_PER_CHUNK,
                TERRAIN_SDF_COLLIDER_DIM,
            )
            .map(Some)
    }

    fn finish_terrain_sdf_solid_sample_chunk(
        &mut self,
        chunk_id: UVec3,
        job: ChunkSolidSampleJob,
    ) -> anyhow::Result<TerrainSdfSourceRefresh> {
        let total_start = Instant::now();
        let sample_result = self
            .plain_builder
            .finish_chunk_atlas_solid_grid_sample(job)?;
        self.store_terrain_sdf_solid_sample_chunk(chunk_id, sample_result, total_start)
    }

    fn store_terrain_sdf_solid_sample_chunk(
        &mut self,
        chunk_id: UVec3,
        sample_result: ChunkSolidSampleResult,
        total_start: Instant,
    ) -> anyhow::Result<TerrainSdfSourceRefresh> {
        let convert_start = Instant::now();
        let voxel_types = sample_result
            .samples
            .iter()
            .map(|&solid| if solid == 0 { 0 } else { 1 })
            .collect::<Vec<u8>>();
        let convert_elapsed = convert_start.elapsed();
        let store_start = Instant::now();
        let (chunk, changed) = self.cpu_solid_voxels.upsert_from_voxel_types(
            chunk_id,
            TERRAIN_SDF_COLLIDER_DIM,
            &voxel_types,
        )?;
        let store_elapsed = store_start.elapsed();
        log::info!(
            "[WATER][TERRAIN] refreshed GPU solid source chunk {:?} rev {} changed={} solid_samples {}/{} source_dim {:?} atlas_offset={:?} atlas_dim={:?} sample_dim={:?} readback_samples={}/{} readback_bytes={} gpu_prepare={:.3}ms gpu_submit={:.3}ms gpu_completion_latency={:.3}ms gpu_readback={:.3}ms gpu_convert={:.3}ms gpu_sample_total={:.3}ms convert={:.3}ms store={:.3}ms total={:.3}ms",
            chunk_id,
            chunk.revision(),
            changed,
            chunk.solid_count(),
            TERRAIN_SDF_COLLIDER_DIM.x as usize
                * TERRAIN_SDF_COLLIDER_DIM.y as usize
                * TERRAIN_SDF_COLLIDER_DIM.z as usize,
            TERRAIN_SDF_COLLIDER_DIM,
            sample_result.atlas_offset,
            sample_result.atlas_dim,
            sample_result.sample_dim,
            sample_result.samples.len(),
            sample_result.sample_count,
            sample_result.byte_count,
            sample_result.prepare_ms,
            sample_result.gpu_submit_ms,
            sample_result.gpu_completion_latency_ms,
            sample_result.readback_ms,
            sample_result.convert_ms,
            sample_result.total_ms,
            convert_elapsed.as_secs_f64() * 1000.0,
            store_elapsed.as_secs_f64() * 1000.0,
            total_start.elapsed().as_secs_f64() * 1000.0,
        );
        Ok(TerrainSdfSourceRefresh {
            revision: chunk.revision(),
            changed,
        })
    }
}

impl TerrainSdfSourceRefreshRequest {
    fn coalesce_keep_earliest_ready(self, pending: Self) -> Self {
        if self.ready_at <= pending.ready_at {
            self
        } else {
            pending
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TerrainSdfSourceRefresh {
    revision: u64,
    changed: bool,
}

fn water_edit_soak_step(step: usize) -> Option<WaterEditSoakStep> {
    match step {
        0 => Some(WaterEditSoakStep {
            delay_sec: 2.0,
            label: "shore-dig-a",
            xz: Vec2::new(1.34, 1.42),
            radius: 0.055,
            op: WaterEditSoakOp::Remove,
        }),
        1 => Some(WaterEditSoakStep {
            delay_sec: 4.0,
            label: "shore-rock-place",
            xz: Vec2::new(1.66, 1.38),
            radius: 0.050,
            op: WaterEditSoakOp::PlaceRock,
        }),
        2 => Some(WaterEditSoakStep {
            delay_sec: 6.0,
            label: "shore-dig-b",
            xz: Vec2::new(1.52, 1.70),
            radius: 0.055,
            op: WaterEditSoakOp::Remove,
        }),
        _ => None,
    }
}

fn build_terrain_sdf_collider_chunk(
    solid_chunk: &CpuSolidVoxelChunk,
    chunk_id: IVec3,
    revision: u64,
) -> Option<TerrainSdfColliderBuild> {
    let build_start = Instant::now();
    let (bounds_min_ws, bounds_max_ws) = water_terrain_chunk_bounds_ws(chunk_id);
    let sample_count = (TERRAIN_SDF_COLLIDER_DIM.x as usize)
        * (TERRAIN_SDF_COLLIDER_DIM.y as usize)
        * (TERRAIN_SDF_COLLIDER_DIM.z as usize);

    let solid_start = Instant::now();
    let (solid_samples, solid_stats) = water_terrain_solid_samples(
        solid_chunk,
        TERRAIN_SDF_COLLIDER_DIM,
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
        TERRAIN_SDF_COLLIDER_DIM,
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

    Some(TerrainSdfColliderBuild {
        chunk: WaterTerrainColliderChunk {
            chunk_id,
            dim: TERRAIN_SDF_COLLIDER_DIM,
            sdf_ws,
            revision,
        },
        bounds_min_ws,
        bounds_max_ws,
        stats: TerrainSdfColliderBuildStats {
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

struct TerrainSdfColliderBuild {
    chunk: WaterTerrainColliderChunk,
    bounds_min_ws: Vec3,
    bounds_max_ws: Vec3,
    stats: TerrainSdfColliderBuildStats,
}

struct TerrainSdfColliderBuildStats {
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

fn terrain_sdf_source_revision(
    cpu_solid_voxels: &crate::app::cpu_solid_voxels::CpuSolidVoxelStore,
    chunk_key: UVec3,
) -> TerrainSdfSourceRevision {
    TerrainSdfSourceRevision {
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

fn water_terrain_chunk_key_intersects_box_grid_domain(
    chunk_key: UVec3,
    box_min_ws: Vec3,
    box_max_ws: Vec3,
) -> bool {
    let Some(chunk_id) = water_terrain_chunk_work_key_to_id(chunk_key) else {
        return false;
    };

    water_terrain_chunk_strictly_overlaps_box(chunk_id, box_min_ws, box_max_ws)
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
    fn water_collider_chunk_bounds_are_unit_aligned() {
        let (min_ws, max_ws) = water_terrain_chunk_bounds_ws(IVec3::ZERO);

        assert_eq!(min_ws, Vec3::ZERO);
        assert_eq!(max_ws, Vec3::ONE);
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
    fn water_chunk_work_key_round_trips_origin_chunk() {
        let chunk_key = water_terrain_chunk_id_to_uvec3(IVec3::ZERO).unwrap();

        assert_eq!(chunk_key, UVec3::ZERO);
        assert_eq!(
            water_terrain_chunk_work_key_to_id(chunk_key),
            Some(IVec3::ZERO)
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
    fn chunk_strict_overlap_accepts_pond_chunk() {
        assert!(water_terrain_chunk_strictly_overlaps_box(
            IVec3::ZERO,
            Vec3::ZERO,
            Vec3::new(2.0, 1.0, 2.0),
        ));
    }

    #[test]
    fn chunk_strict_overlap_rejects_only_touching_neighbor() {
        assert!(!water_terrain_chunk_strictly_overlaps_box(
            IVec3::new(2, 0, 1),
            Vec3::ZERO,
            Vec3::new(2.0, 1.0, 2.0),
        ));
        assert!(!water_terrain_chunk_strictly_overlaps_box(
            IVec3::new(1, 1, 1),
            Vec3::ZERO,
            Vec3::new(2.0, 1.0, 2.0),
        ));
    }

    #[test]
    fn chunk_grid_domain_uses_strict_overlap() {
        let min_ws = Vec3::ZERO;
        let max_ws = Vec3::new(2.0, 1.0, 2.0);

        assert!(water_terrain_chunk_key_intersects_box_grid_domain(
            UVec3::ZERO,
            min_ws,
            max_ws,
        ));
        assert!(water_terrain_chunk_key_intersects_box_grid_domain(
            UVec3::new(1, 0, 1),
            min_ws,
            max_ws,
        ));
        assert!(!water_terrain_chunk_key_intersects_box_grid_domain(
            UVec3::new(2, 1, 2),
            min_ws,
            max_ws,
        ));
        assert!(!water_terrain_chunk_key_intersects_box_grid_domain(
            UVec3::new(2, 0, 1),
            min_ws,
            max_ws,
        ));
        assert!(!water_terrain_chunk_key_intersects_box_grid_domain(
            UVec3::new(1, 1, 1),
            min_ws,
            max_ws,
        ));
    }

    #[test]
    fn source_revision_depends_only_on_same_chunk() {
        let mut store = crate::app::cpu_solid_voxels::CpuSolidVoxelStore::default();
        store
            .upsert_from_voxel_types(UVec3::new(1, 0, 1), UVec3::ONE, &[7])
            .unwrap();

        assert_eq!(
            terrain_sdf_source_revision(&store, UVec3::new(1, 0, 1)),
            TerrainSdfSourceRevision {
                dependencies: vec![(UVec3::new(1, 0, 1), 1)]
            }
        );
        assert_eq!(
            terrain_sdf_source_revision(&store, UVec3::new(1, 1, 1)),
            TerrainSdfSourceRevision {
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
