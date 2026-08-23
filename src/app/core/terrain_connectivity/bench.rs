//! PROTOTYPE: deterministic release-mode workload for the oversized detached-terrain policy.
//!
//! Question: can the current atlas/publication architecture classify and atomically invalidate one
//! 437,205-voxel detached hollow canopy, then represent at most 16,384 voxels as particles, without
//! exceeding interactive frame budgets? This module is activated only by the diagnostic CLI.

use super::*;
use crate::cli::{TerrainConnectivityBenchMode, TerrainConnectivityBenchOptions};
use crate::particles::{
    MotionMode, ParticleRenderKind, ParticleSpawn, ParticleSystem, ParticleUpdateConfig,
    PARTICLE_CAPACITY,
};
use anyhow::Context;
use glam::{UVec3, Vec3, Vec4};
use re_flora_vkn::GpuProfilerFrameResults;
use std::collections::VecDeque;
use std::time::Instant;

const FIXTURE_ORIGIN: UVec3 = UVec3::new(96, 145, 72);
const FIXTURE_DIM: UVec3 = UVec3::new(187, 55, 243);
const FIXTURE_THICKNESS: u32 = 5;
pub(super) const FIXTURE_VOXELS: usize = 437_205;
const PRE_EVENT_FRAME_SAMPLES: usize = 120;

#[derive(Clone, Copy, Debug, PartialEq)]
enum BenchState {
    InstallFixture,
    Warmup {
        ready_after_frame: u64,
    },
    Tracing {
        release_frame: u64,
        revision_before: u32,
        snapshot_readback_us: f64,
        classification_us: f64,
    },
    ValidateAtomicity {
        release_frame: u64,
        revision_before: u32,
        snapshot_readback_us: f64,
        classification_us: f64,
    },
    Commit {
        release_frame: u64,
        revision_before: u32,
        snapshot_readback_us: f64,
        classification_us: f64,
        atomic_validation_us: f64,
        sampling_us: f64,
        staging_clear_us: f64,
        sampled_voxels: usize,
    },
    Observing {
        event_frame: u64,
    },
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundedDisposition {
    Pending,
    Detached,
    Anchored,
    Deferred,
}

impl BoundedDisposition {
    fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Detached => "detached",
            Self::Anchored => "anchored",
            Self::Deferred => "deferred",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct BoundedStep {
    disposition: BoundedDisposition,
    processed: usize,
}

struct BoundedTopologyJob {
    bound: UAabb3,
    dim: UVec3,
    snapshot: Vec<u8>,
    visited: Vec<bool>,
    queue: VecDeque<u32>,
    component: Vec<u32>,
    terminal: Option<BoundedDisposition>,
}

impl BoundedTopologyJob {
    fn new(bound: UAabb3, snapshot: Vec<u8>, seed: UVec3) -> anyhow::Result<Self> {
        let dim = bound.dimensions();
        let expected = usize::try_from(voxel_count(bound))?;
        anyhow::ensure!(snapshot.len() == expected);
        anyhow::ensure!(seed.cmpge(bound.min()).all() && seed.cmplt(bound.max()).all());
        let local = seed - bound.min();
        let seed_index = local.x + dim.x * (local.y + dim.y * local.z);
        anyhow::ensure!(snapshot[seed_index as usize] & VOXEL_TYPE_MASK as u8 != 0);
        let mut visited = vec![false; expected];
        visited[seed_index as usize] = true;
        Ok(Self {
            bound,
            dim,
            snapshot,
            visited,
            queue: VecDeque::from([seed_index]),
            component: Vec::with_capacity(FIXTURE_VOXELS),
            terminal: None,
        })
    }

    fn advance(&mut self, budget: usize) -> BoundedStep {
        if let Some(disposition) = self.terminal {
            return BoundedStep {
                disposition,
                processed: 0,
            };
        }
        let mut processed = 0;
        while processed < budget {
            let Some(index) = self.queue.pop_front() else {
                self.terminal = Some(BoundedDisposition::Detached);
                break;
            };
            processed += 1;
            self.component.push(index);
            let local = self.position_of(index);
            let world = self.bound.min() + local;
            if world.y == 0 {
                self.terminal = Some(BoundedDisposition::Anchored);
                break;
            }
            if local.x == 0
                || local.y == 0
                || local.z == 0
                || local.x + 1 == self.dim.x
                || local.y + 1 == self.dim.y
                || local.z + 1 == self.dim.z
            {
                self.terminal = Some(BoundedDisposition::Deferred);
                break;
            }

            let neighbors = [
                local - UVec3::X,
                local + UVec3::X,
                local - UVec3::Y,
                local + UVec3::Y,
                local - UVec3::Z,
                local + UVec3::Z,
            ];
            for neighbor in neighbors {
                let neighbor_index = self.index_of(neighbor);
                if !self.visited[neighbor_index as usize]
                    && self.snapshot[neighbor_index as usize] & VOXEL_TYPE_MASK as u8 != 0
                {
                    self.visited[neighbor_index as usize] = true;
                    self.queue.push_back(neighbor_index);
                }
            }
        }
        let disposition = self.terminal.unwrap_or(BoundedDisposition::Pending);
        BoundedStep {
            disposition,
            processed,
        }
    }

    fn index_of(&self, local: UVec3) -> u32 {
        local.x + self.dim.x * (local.y + self.dim.y * local.z)
    }

    fn position_of(&self, index: u32) -> UVec3 {
        let plane = self.dim.x * self.dim.y;
        let z = index / plane;
        let remainder = index % plane;
        UVec3::new(remainder % self.dim.x, remainder / self.dim.x, z)
    }

    fn pending_len(&self) -> usize {
        self.queue.len()
    }

    fn component_len(&self) -> usize {
        self.component.len()
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct QueueHighWater {
    terrain_collider: usize,
    contree_cache: usize,
    water_source: usize,
    water_collider: usize,
    water_cache: usize,
}

#[derive(Clone, Copy, Debug)]
struct CpuFrameRecord {
    frame: u64,
    total_us: f32,
    gpu_present_us: f32,
    tracked_us: f32,
    untracked_us: f32,
    terrain_collider_pending: usize,
    contree_cache_pending: usize,
    water_source_pending: usize,
    water_collider_pending: usize,
    water_cache_pending: usize,
    ddgi_ready: bool,
    visible_revision: u32,
}

#[derive(Clone, Copy, Debug)]
struct GpuFrameRecord {
    source_frame: u64,
    render_us: f64,
    tracer_us: f64,
    scopes: usize,
    dropped: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct EventStages {
    total_us: f64,
    current_path_us: f64,
    primary_readback_us: f64,
    trace_readback_us: f64,
    classification_us: f64,
    sampling_us: f64,
    staging_clear_us: f64,
    invalidation_us: f64,
    publication_us: f64,
    particle_spawn_us: f64,
    classified_voxels: usize,
    trace_readback_tiles: usize,
    invalidated_voxels: usize,
    sampled_voxels: usize,
    spawned_particles: usize,
    revision_before: u32,
    revision_after: u32,
    release_to_commit_frames: u64,
    atomic_validation_us: f64,
}

pub(in crate::app::core) struct TerrainConnectivityBench {
    options: TerrainConnectivityBenchOptions,
    state: BenchState,
    pre_event_cpu: VecDeque<CpuFrameRecord>,
    pre_event_gpu: VecDeque<GpuFrameRecord>,
    gpu_source_frame_by_slot: Vec<Option<u64>>,
    high_water: QueueHighWater,
    stages: Option<EventStages>,
    bounded_job: Option<BoundedTopologyJob>,
    pending_visual_voxels: Option<Vec<(UVec3, u8)>>,
}

impl TerrainConnectivityBench {
    pub(in crate::app::core) fn new(options: TerrainConnectivityBenchOptions) -> Self {
        Self {
            options,
            state: BenchState::InstallFixture,
            pre_event_cpu: VecDeque::with_capacity(PRE_EVENT_FRAME_SAMPLES),
            pre_event_gpu: VecDeque::with_capacity(PRE_EVENT_FRAME_SAMPLES),
            gpu_source_frame_by_slot: Vec::new(),
            high_water: QueueHighWater::default(),
            stages: None,
            bounded_job: None,
            pending_visual_voxels: None,
        }
    }

    pub(in crate::app::core) fn active(&self) -> bool {
        self.state != BenchState::Complete
    }

    pub(in crate::app::core) fn advance(app: &mut App) -> anyhow::Result<()> {
        let Some(mut bench) = app.terrain_connectivity_bench.take() else {
            return Ok(());
        };
        let result = bench.advance_inner(app);
        app.terrain_connectivity_bench = Some(bench);
        result
    }

    fn advance_inner(&mut self, app: &mut App) -> anyhow::Result<()> {
        let frame = app.time_info.total_frame_count();
        match self.state {
            BenchState::InstallFixture => {
                let started = Instant::now();
                install_fixture(app)?;
                let reserve_started = Instant::now();
                set_available_particle_capacity(app, self.options.available_particles)?;
                let reserve_us = reserve_started.elapsed().as_secs_f64() * 1_000_000.0;
                let ready_after_frame = frame.saturating_add(u64::from(self.options.warmup_frames));
                self.state = BenchState::Warmup { ready_after_frame };
                log::info!(
                    "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=fixture mode={} frame={} fixture_voxels={} fixture_min={:?} fixture_max={:?} affected_chunks=4 available_particles={} reserve_us={:.0} setup_us={:.0} warmup_frames={} observe_frames={} revision={}",
                    self.options.mode.label(),
                    frame,
                    FIXTURE_VOXELS,
                    fixture_bound().min(),
                    fixture_bound().max(),
                    app.particle_system.available_capacity(),
                    reserve_us,
                    started.elapsed().as_secs_f64() * 1_000_000.0,
                    self.options.warmup_frames,
                    self.options.observe_frames,
                    app.visible_terrain_revision,
                );
            }
            BenchState::Warmup { ready_after_frame } => {
                let ready = frame >= ready_after_frame
                    && app.contree_builder.cpu_chunk_cache_jobs_idle()
                    && app.terrain_physics.terrain_collider_pending_len() == 0
                    && app.water_terrain_status().is_ready()
                    && app
                        .tracer
                        .ddgi_ready_for_terrain_revision(app.visible_terrain_revision);
                if ready {
                    anyhow::ensure!(
                        app.particle_system.available_capacity()
                            == self.options.available_particles,
                        "terrain connectivity benchmark particle availability drifted: actual={} expected={}",
                        app.particle_system.available_capacity(),
                        self.options.available_particles,
                    );
                    if self.options.mode == TerrainConnectivityBenchMode::Bounded {
                        let revision_before = app.visible_terrain_revision;
                        let snapshot_started = Instant::now();
                        let bound = isolation_bound();
                        let snapshot = app
                            .plain_builder
                            .read_chunk_atlas_region(bound.min(), bound.dimensions())?;
                        let snapshot_readback_us =
                            snapshot_started.elapsed().as_secs_f64() * 1_000_000.0;
                        self.bounded_job =
                            Some(BoundedTopologyJob::new(bound, snapshot, FIXTURE_ORIGIN)?);
                        self.state = BenchState::Tracing {
                            release_frame: frame,
                            revision_before,
                            snapshot_readback_us,
                            classification_us: 0.0,
                        };
                        log::info!(
                            "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=job_start mode=bounded release_frame={} revision={} snapshot_voxels={} snapshot_readback_us={:.0} voxel_budget={} available_particles={}",
                            frame,
                            revision_before,
                            voxel_count(bound),
                            snapshot_readback_us,
                            self.options.voxel_budget,
                            app.particle_system.available_capacity(),
                        );
                    } else {
                        let event_frame = frame;
                        let stages = run_release_event(app, self.options.mode)?;
                        self.stages = Some(stages);
                        self.state = BenchState::Observing { event_frame };
                        log_event(self.options, event_frame, stages);
                    }
                } else if frame.is_multiple_of(120) {
                    log::info!(
                        "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=warmup frame={} ready_after={} contree_idle={} terrain_collider_pending={} water_ready={} ddgi_ready={} revision={}",
                        frame,
                        ready_after_frame,
                        app.contree_builder.cpu_chunk_cache_jobs_idle(),
                        app.terrain_physics.terrain_collider_pending_len(),
                        app.water_terrain_status().is_ready(),
                        app.tracer
                            .ddgi_ready_for_terrain_revision(app.visible_terrain_revision),
                        app.visible_terrain_revision,
                    );
                }
            }
            BenchState::Tracing {
                release_frame,
                revision_before,
                snapshot_readback_us,
                mut classification_us,
            } => {
                anyhow::ensure!(
                    app.visible_terrain_revision == revision_before,
                    "bounded topology input revision changed while pending"
                );
                let step_started = Instant::now();
                let job = self
                    .bounded_job
                    .as_mut()
                    .context("bounded topology state lost its job")?;
                let step = job.advance(self.options.voxel_budget);
                let step_us = step_started.elapsed().as_secs_f64() * 1_000_000.0;
                classification_us += step_us;
                log::info!(
                    "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=job_frame mode=bounded release_frame={} frame={} relative={} voxel_budget={} processed={} processed_total={} pending={} step_us={:.0} classification_us={:.0} disposition={} visible_revision={}",
                    release_frame,
                    frame,
                    frame.saturating_sub(release_frame),
                    self.options.voxel_budget,
                    step.processed,
                    job.component_len(),
                    job.pending_len(),
                    step_us,
                    classification_us,
                    step.disposition.label(),
                    app.visible_terrain_revision,
                );
                match step.disposition {
                    BoundedDisposition::Pending => {
                        self.state = BenchState::Tracing {
                            release_frame,
                            revision_before,
                            snapshot_readback_us,
                            classification_us,
                        };
                    }
                    BoundedDisposition::Detached => {
                        self.state = BenchState::ValidateAtomicity {
                            release_frame,
                            revision_before,
                            snapshot_readback_us,
                            classification_us,
                        };
                    }
                    BoundedDisposition::Anchored | BoundedDisposition::Deferred => {
                        anyhow::bail!(
                            "bounded detached fixture ended as {}",
                            step.disposition.label()
                        );
                    }
                }
            }
            BenchState::ValidateAtomicity {
                release_frame,
                revision_before,
                snapshot_readback_us,
                classification_us,
            } => {
                anyhow::ensure!(app.visible_terrain_revision == revision_before);
                let validation_started = Instant::now();
                let before_commit = count_fixture_solids(app)?;
                let atomic_validation_us = validation_started.elapsed().as_secs_f64() * 1_000_000.0;
                anyhow::ensure!(
                    before_commit == FIXTURE_VOXELS,
                    "bounded topology modified live terrain while pending: remaining={before_commit}"
                );
                log::info!(
                    "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=atomic_check mode=bounded release_frame={} frame={} remaining_fixture_voxels={} visible_revision={} validation_us={:.0}",
                    release_frame,
                    frame,
                    before_commit,
                    app.visible_terrain_revision,
                    atomic_validation_us,
                );
                anyhow::ensure!(
                    app.particle_system.available_capacity() == self.options.available_particles
                );
                let job = self
                    .bounded_job
                    .as_mut()
                    .context("bounded topology validation lost its job")?;
                let (visual_voxels, sampling_us, staging_clear_us) =
                    prepare_bounded_commit(job, self.options.available_particles);
                let sampled_voxels = visual_voxels.len();
                self.pending_visual_voxels = Some(visual_voxels);
                self.state = BenchState::Commit {
                    release_frame,
                    revision_before,
                    snapshot_readback_us,
                    classification_us,
                    atomic_validation_us,
                    sampling_us,
                    staging_clear_us,
                    sampled_voxels,
                };
            }
            BenchState::Commit {
                release_frame,
                revision_before,
                snapshot_readback_us,
                classification_us,
                atomic_validation_us,
                sampling_us,
                staging_clear_us,
                sampled_voxels,
            } => {
                let event_frame = frame;
                let stages = run_bounded_commit(
                    app,
                    self.bounded_job
                        .take()
                        .context("bounded topology commit lost its job")?,
                    release_frame,
                    revision_before,
                    snapshot_readback_us,
                    classification_us,
                    atomic_validation_us,
                    sampling_us,
                    staging_clear_us,
                    sampled_voxels,
                )?;
                self.stages = Some(stages);
                self.state = BenchState::Observing { event_frame };
                log_event(self.options, event_frame, stages);
            }
            BenchState::Observing { event_frame } => {
                if let Some(visual_voxels) = self.pending_visual_voxels.take() {
                    let particle_started = Instant::now();
                    let spawned = app.spawn_detached_terrain_voxel_particles(&visual_voxels);
                    let particle_spawn_us = particle_started.elapsed().as_secs_f64() * 1_000_000.0;
                    anyhow::ensure!(spawned == visual_voxels.len());
                    let stages = self
                        .stages
                        .as_mut()
                        .context("bounded visual spawn lost event stages")?;
                    stages.particle_spawn_us = particle_spawn_us;
                    stages.spawned_particles = spawned;
                    log::info!(
                        "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=visual_spawn mode=bounded event_frame={} frame={} relative={} spawned_particles={} particle_spawn_us={:.0} visible_revision={}",
                        event_frame,
                        frame,
                        frame.saturating_sub(event_frame),
                        spawned,
                        particle_spawn_us,
                        app.visible_terrain_revision,
                    );
                }
            }
            BenchState::Complete => {}
        }
        Ok(())
    }

    pub(in crate::app::core) fn gpu_source_frame(&self, frame_slot: usize) -> Option<u64> {
        self.gpu_source_frame_by_slot
            .get(frame_slot)
            .copied()
            .flatten()
    }

    pub(in crate::app::core) fn note_gpu_frame_started(&mut self, frame_slot: usize, frame: u64) {
        if self.gpu_source_frame_by_slot.len() <= frame_slot {
            self.gpu_source_frame_by_slot.resize(frame_slot + 1, None);
        }
        self.gpu_source_frame_by_slot[frame_slot] = Some(frame);
    }

    pub(in crate::app::core) fn observe_gpu_results(
        &mut self,
        source_frame: Option<u64>,
        results: &GpuProfilerFrameResults,
    ) {
        let Some(source_frame) = source_frame else {
            return;
        };
        let duration = |name: &str| {
            results
                .scopes
                .iter()
                .find(|scope| scope.name == name)
                .map_or(0.0, |scope| scope.duration_us())
        };
        let record = GpuFrameRecord {
            source_frame,
            render_us: duration("frame.render"),
            tracer_us: duration("tracer.render"),
            scopes: results.scopes.len(),
            dropped: results.dropped_scope_count as usize,
        };
        match self.state {
            BenchState::InstallFixture
            | BenchState::Warmup { .. }
            | BenchState::Tracing { .. }
            | BenchState::ValidateAtomicity { .. }
            | BenchState::Commit { .. } => {
                push_bounded(&mut self.pre_event_gpu, record);
            }
            BenchState::Observing { event_frame } => {
                if source_frame >= event_frame {
                    log_gpu_frame(event_frame, record);
                }
            }
            BenchState::Complete => {}
        }
    }

    pub(in crate::app::core) fn observe_completed_frame(
        &mut self,
        app: &mut App,
        timing: super::super::frame_timing::FrameTimingSnapshot,
    ) -> anyhow::Result<bool> {
        let water = app.water_terrain_status().diagnostics();
        let record = CpuFrameRecord {
            frame: timing.frame,
            total_us: timing.total_ms * 1_000.0,
            gpu_present_us: timing.gpu_present_ms * 1_000.0,
            tracked_us: timing.tracked_cpu_ms * 1_000.0,
            untracked_us: timing.untracked_cpu_ms * 1_000.0,
            terrain_collider_pending: app.terrain_physics.terrain_collider_pending_len(),
            contree_cache_pending: app.contree_builder.cpu_chunk_cache_pending_len(),
            water_source_pending: water.source_pending + water.source_active,
            water_collider_pending: water.collider_pending + water.collider_active,
            water_cache_pending: water.cache_pending + water.cache_active,
            ddgi_ready: app
                .tracer
                .ddgi_ready_for_terrain_revision(app.visible_terrain_revision),
            visible_revision: app.visible_terrain_revision,
        };
        match self.state {
            BenchState::InstallFixture
            | BenchState::Warmup { .. }
            | BenchState::Tracing { .. }
            | BenchState::ValidateAtomicity { .. }
            | BenchState::Commit { .. } => {
                push_bounded(&mut self.pre_event_cpu, record);
                Ok(false)
            }
            BenchState::Observing { event_frame } => {
                self.high_water.terrain_collider = self
                    .high_water
                    .terrain_collider
                    .max(record.terrain_collider_pending);
                self.high_water.contree_cache = self
                    .high_water
                    .contree_cache
                    .max(record.contree_cache_pending);
                self.high_water.water_source = self
                    .high_water
                    .water_source
                    .max(record.water_source_pending);
                self.high_water.water_collider = self
                    .high_water
                    .water_collider
                    .max(record.water_collider_pending);
                self.high_water.water_cache =
                    self.high_water.water_cache.max(record.water_cache_pending);
                if record.frame == event_frame {
                    for prior in self.pre_event_cpu.drain(..) {
                        log_cpu_frame(event_frame, prior);
                    }
                    for prior in self.pre_event_gpu.drain(..) {
                        log_gpu_frame(event_frame, prior);
                    }
                }
                log_cpu_frame(event_frame, record);
                if record.frame.saturating_sub(event_frame)
                    >= u64::from(self.options.observe_frames)
                {
                    let remaining = count_fixture_solids(app)?;
                    let expected = match self.options.mode {
                        TerrainConnectivityBenchMode::Existing => FIXTURE_VOXELS,
                        TerrainConnectivityBenchMode::Correct
                        | TerrainConnectivityBenchMode::Bounded => 0,
                    };
                    anyhow::ensure!(
                        remaining == expected,
                        "terrain connectivity benchmark exposed a partial fixture: remaining={remaining} expected={expected}"
                    );
                    let stages = self
                        .stages
                        .context("terrain connectivity benchmark completed without event stages")?;
                    log::info!(
                        "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=summary mode={} event_frame={} observed_frames={} remaining_fixture_voxels={} disposition={} invalidated_voxels={} spawned_particles={} revision_before={} revision_after={} high_water_terrain_collider={} high_water_contree_cache={} high_water_water_source={} high_water_water_collider={} high_water_water_cache={} ddgi_ready={}",
                        self.options.mode.label(),
                        event_frame,
                        self.options.observe_frames,
                        remaining,
                        if self.options.mode == TerrainConnectivityBenchMode::Existing {
                            "capacity_rejected"
                        } else {
                            "detached"
                        },
                        stages.invalidated_voxels,
                        stages.spawned_particles,
                        stages.revision_before,
                        stages.revision_after,
                        self.high_water.terrain_collider,
                        self.high_water.contree_cache,
                        self.high_water.water_source,
                        self.high_water.water_collider,
                        self.high_water.water_cache,
                        record.ddgi_ready,
                    );
                    self.state = BenchState::Complete;
                    return Ok(true);
                }
                Ok(false)
            }
            BenchState::Complete => Ok(true),
        }
    }
}

fn run_release_event(
    app: &mut App,
    mode: TerrainConnectivityBenchMode,
) -> anyhow::Result<EventStages> {
    let total_started = Instant::now();
    let revision_before = app.visible_terrain_revision;
    app.terrain_connectivity = TerrainConnectivityRuntime::default();
    app.terrain_connectivity.record_edit(fixture_edit_bound());

    if mode == TerrainConnectivityBenchMode::Existing {
        let current_started = Instant::now();
        app.resolve_detached_terrain_after_edit()?;
        return Ok(EventStages {
            total_us: total_started.elapsed().as_secs_f64() * 1_000_000.0,
            current_path_us: current_started.elapsed().as_secs_f64() * 1_000_000.0,
            revision_before,
            revision_after: app.visible_terrain_revision,
            ..EventStages::default()
        });
    }

    let world_dim = CHUNK_DIM * VOXEL_DIM_PER_CHUNK;
    let (edited, block) = app
        .terrain_connectivity
        .take_edit_region(world_dim)
        .context("bench edit region disappeared")?;
    let primary_started = Instant::now();
    let atlas_voxels = app
        .plain_builder
        .read_chunk_atlas_region(block.min(), block.dimensions())?;
    let primary_readback_us = primary_started.elapsed().as_secs_f64() * 1_000_000.0;
    let candidate_region = UAabb3::new(
        edited.min().saturating_sub(UVec3::ONE),
        edited.max().saturating_add(UVec3::ONE).min(world_dim),
    );

    let classification_started = Instant::now();
    let (mut components, trace_readback_us, trace_readback_tiles) = {
        let mut reader =
            AtlasVoxelReader::new(&mut app.plain_builder, world_dim, block, &atlas_voxels);
        let components = detached_components_in_edit_region(
            &atlas_voxels,
            block,
            candidate_region,
            world_dim,
            VOXEL_TYPE_MASK as u8,
            usize::MAX,
            |world_voxel| reader.voxel_at(world_voxel),
        )?;
        (components, reader.tile_readback_us, reader.tiles.len())
    };
    let classification_us = classification_started.elapsed().as_secs_f64() * 1_000_000.0;
    anyhow::ensure!(
        components.len() == 1,
        "expected one detached fixture component"
    );
    let component = components.pop().expect("one component was checked");
    anyhow::ensure!(
        component.voxels.len() == FIXTURE_VOXELS,
        "fixture classification returned {} voxels, expected {}",
        component.voxels.len(),
        FIXTURE_VOXELS,
    );

    let sampling_started = Instant::now();
    let visual_voxels =
        deterministic_visual_sample(&component.voxels, app.particle_system.available_capacity());
    let sampling_us = sampling_started.elapsed().as_secs_f64() * 1_000_000.0;

    let invalidation_started = Instant::now();
    clear_detached_voxels(&mut app.plain_builder, world_dim, &component.voxels)?;
    let invalidation_us = invalidation_started.elapsed().as_secs_f64() * 1_000_000.0;

    let publication_started = Instant::now();
    let change = VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
        fixture_bound(),
    )])?
    .context("fixture invalidation has no visible terrain chunks")?;
    app.publish_visible_terrain(change)?;
    let publication_us = publication_started.elapsed().as_secs_f64() * 1_000_000.0;

    let particle_started = Instant::now();
    let spawned_particles = app.spawn_detached_terrain_voxel_particles(&visual_voxels);
    let particle_spawn_us = particle_started.elapsed().as_secs_f64() * 1_000_000.0;
    anyhow::ensure!(
        spawned_particles == visual_voxels.len(),
        "bench sampled {} visual voxels but spawned {} particles",
        visual_voxels.len(),
        spawned_particles,
    );

    Ok(EventStages {
        total_us: total_started.elapsed().as_secs_f64() * 1_000_000.0,
        primary_readback_us,
        trace_readback_us,
        classification_us,
        sampling_us,
        invalidation_us,
        publication_us,
        particle_spawn_us,
        classified_voxels: component.voxels.len(),
        trace_readback_tiles,
        invalidated_voxels: component.voxels.len(),
        sampled_voxels: visual_voxels.len(),
        spawned_particles,
        revision_before,
        revision_after: app.visible_terrain_revision,
        ..EventStages::default()
    })
}

fn run_bounded_commit(
    app: &mut App,
    job: BoundedTopologyJob,
    release_frame: u64,
    revision_before: u32,
    snapshot_readback_us: f64,
    classification_us: f64,
    atomic_validation_us: f64,
    sampling_us: f64,
    staging_clear_us: f64,
    sampled_voxels: usize,
) -> anyhow::Result<EventStages> {
    let total_started = Instant::now();
    anyhow::ensure!(
        app.visible_terrain_revision == revision_before,
        "bounded topology commit revision changed from {} to {}",
        revision_before,
        app.visible_terrain_revision,
    );
    anyhow::ensure!(
        job.terminal == Some(BoundedDisposition::Detached) && job.component.len() == FIXTURE_VOXELS,
        "bounded topology commit is not one complete detached fixture"
    );
    let invalidation_started = Instant::now();
    app.plain_builder.write_chunk_atlas_region(
        job.bound.min(),
        job.bound.dimensions(),
        &job.snapshot,
    )?;
    let invalidation_us = invalidation_started.elapsed().as_secs_f64() * 1_000_000.0;

    let publication_started = Instant::now();
    let change = VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
        fixture_bound(),
    )])?
    .context("bounded fixture invalidation has no visible terrain chunks")?;
    app.publish_visible_terrain(change)?;
    let publication_us = publication_started.elapsed().as_secs_f64() * 1_000_000.0;

    Ok(EventStages {
        total_us: total_started.elapsed().as_secs_f64() * 1_000_000.0,
        primary_readback_us: snapshot_readback_us,
        classification_us,
        sampling_us,
        staging_clear_us,
        invalidation_us,
        publication_us,
        classified_voxels: job.component.len(),
        invalidated_voxels: job.component.len(),
        sampled_voxels,
        revision_before,
        revision_after: app.visible_terrain_revision,
        release_to_commit_frames: app
            .time_info
            .total_frame_count()
            .saturating_sub(release_frame),
        atomic_validation_us,
        ..EventStages::default()
    })
}

fn prepare_bounded_commit(
    job: &mut BoundedTopologyJob,
    available_particles: usize,
) -> (Vec<(UVec3, u8)>, f64, f64) {
    let sampling_started = Instant::now();
    let sampled_count = available_particles.min(job.component.len());
    let visual_voxels = (0..sampled_count)
        .map(|sample| {
            let index = job.component[sample * job.component.len() / sampled_count];
            let world = job.bound.min() + job.position_of(index);
            (world, job.snapshot[index as usize] & VOXEL_TYPE_MASK as u8)
        })
        .collect::<Vec<_>>();
    let sampling_us = sampling_started.elapsed().as_secs_f64() * 1_000_000.0;

    let clear_started = Instant::now();
    for &index in &job.component {
        job.snapshot[index as usize] = 0;
    }
    let staging_clear_us = clear_started.elapsed().as_secs_f64() * 1_000_000.0;
    (visual_voxels, sampling_us, staging_clear_us)
}

fn install_fixture(app: &mut App) -> anyhow::Result<()> {
    let isolation = isolation_bound();
    let isolation_data = vec![0; voxel_count(isolation) as usize];
    app.plain_builder.write_chunk_atlas_region(
        isolation.min(),
        isolation.dimensions(),
        &isolation_data,
    )?;
    let fixture = generate_hollow_canopy();
    app.plain_builder.write_chunk_atlas_region(
        fixture_bound().min(),
        fixture_bound().dimensions(),
        &fixture,
    )?;
    app.plain_builder.mark_all_solid_workgroups_dirty();
    let change = VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
        isolation,
    )])?
    .context("fixture installation has no visible terrain chunks")?;
    app.publish_visible_terrain(change)?;
    anyhow::ensure!(count_fixture_solids(app)? == FIXTURE_VOXELS);
    Ok(())
}

fn generate_hollow_canopy() -> Vec<u8> {
    let mut voxels = vec![0; (FIXTURE_DIM.x * FIXTURE_DIM.y * FIXTURE_DIM.z) as usize];
    let mut count = 0usize;
    for z in 0..FIXTURE_DIM.z {
        for y in 0..FIXTURE_DIM.y {
            for x in 0..FIXTURE_DIM.x {
                let solid = x < FIXTURE_THICKNESS
                    || x + FIXTURE_THICKNESS >= FIXTURE_DIM.x
                    || z < FIXTURE_THICKNESS
                    || z + FIXTURE_THICKNESS >= FIXTURE_DIM.z
                    || y + FIXTURE_THICKNESS >= FIXTURE_DIM.y;
                if solid {
                    let index = x + FIXTURE_DIM.x * (y + FIXTURE_DIM.y * z);
                    voxels[index as usize] = crate::builder::VOXEL_TYPE_ROCK as u8;
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, FIXTURE_VOXELS);
    voxels
}

fn deterministic_visual_sample(voxels: &[(UVec3, u8)], capacity: usize) -> Vec<(UVec3, u8)> {
    let count = capacity.min(voxels.len());
    if count == voxels.len() {
        return voxels.to_vec();
    }
    (0..count)
        .map(|sample| voxels[sample * voxels.len() / count])
        .collect()
}

fn set_available_particle_capacity(app: &mut App, available: usize) -> anyhow::Result<()> {
    anyhow::ensure!(available <= PARTICLE_CAPACITY);
    app.particle_system = ParticleSystem::new(PARTICLE_CAPACITY);
    app.particle_snapshots.clear();
    app.terrain_harvest_particle_handles.clear();
    let reserved = PARTICLE_CAPACITY - available;
    for index in 0..reserved {
        let handle = app.particle_system.spawn(ParticleSpawn {
            position: Vec3::splat(-10.0),
            velocity: Vec3::ZERO,
            color: Vec4::ZERO,
            size: 0.0,
            lifetime: f32::MAX,
            wind_factor: 0.0,
            gravity_factor: 0.0,
            drift_direction: Vec3::ZERO,
            drift_strength: 0.0,
            drift_frequency: 0.0,
            speed_noise_offset: index as f32,
            motion_mode: MotionMode::Free,
            sink_on_lifetime: false,
            sink_speed: 0.0,
            texture_variant: 0,
            render_kind: ParticleRenderKind::TerrainVoxel,
            despawn_on_lifetime: false,
            despawn_below_ground: false,
            update: ParticleUpdateConfig::new(60.0, 1),
        });
        anyhow::ensure!(handle.is_some(), "failed to reserve particle slot {index}");
    }
    anyhow::ensure!(app.particle_system.available_capacity() == available);
    Ok(())
}

fn count_fixture_solids(app: &mut App) -> anyhow::Result<usize> {
    let bound = fixture_bound();
    let voxels = app
        .plain_builder
        .read_chunk_atlas_region(bound.min(), bound.dimensions())?;
    Ok(voxels
        .iter()
        .filter(|voxel| **voxel & VOXEL_TYPE_MASK as u8 != 0)
        .count())
}

fn fixture_bound() -> UAabb3 {
    UAabb3::new(FIXTURE_ORIGIN, FIXTURE_ORIGIN + FIXTURE_DIM)
}

fn isolation_bound() -> UAabb3 {
    UAabb3::new(
        FIXTURE_ORIGIN - UVec3::ONE,
        FIXTURE_ORIGIN + FIXTURE_DIM + UVec3::ONE,
    )
}

fn fixture_edit_bound() -> UAabb3 {
    let adjacent = UVec3::new(
        FIXTURE_ORIGIN.x,
        FIXTURE_ORIGIN.y - 1,
        FIXTURE_ORIGIN.z + FIXTURE_DIM.z / 2,
    );
    UAabb3::new(adjacent, adjacent)
}

fn log_event(options: TerrainConnectivityBenchOptions, frame: u64, stages: EventStages) {
    log::info!(
        "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=event mode={} frame={} available_particles={} voxel_budget={} fixture_voxels={} total_us={:.0} current_path_us={:.0} primary_readback_us={:.0} trace_readback_us={:.0} classification_us={:.0} atomic_validation_us={:.0} sampling_us={:.0} staging_clear_us={:.0} invalidation_us={:.0} publication_us={:.0} particle_spawn_us={:.0} classified_voxels={} trace_readback_tiles={} invalidated_voxels={} sampled_voxels={} spawned_particles={} revision_before={} revision_after={} release_to_commit_frames={}",
        options.mode.label(),
        frame,
        options.available_particles,
        options.voxel_budget,
        FIXTURE_VOXELS,
        stages.total_us,
        stages.current_path_us,
        stages.primary_readback_us,
        stages.trace_readback_us,
        stages.classification_us,
        stages.atomic_validation_us,
        stages.sampling_us,
        stages.staging_clear_us,
        stages.invalidation_us,
        stages.publication_us,
        stages.particle_spawn_us,
        stages.classified_voxels,
        stages.trace_readback_tiles,
        stages.invalidated_voxels,
        stages.sampled_voxels,
        stages.spawned_particles,
        stages.revision_before,
        stages.revision_after,
        stages.release_to_commit_frames,
    );
}

fn log_cpu_frame(event_frame: u64, record: CpuFrameRecord) {
    let relative = record.frame as i64 - event_frame as i64;
    log::info!(
        "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=frame frame={} relative={} cpu_total_us={:.0} gpu_present_us={:.0} tracked_us={:.0} untracked_us={:.0} terrain_collider_pending={} contree_cache_pending={} water_source_pending={} water_collider_pending={} water_cache_pending={} ddgi_ready={} visible_revision={}",
        record.frame,
        relative,
        record.total_us,
        record.gpu_present_us,
        record.tracked_us,
        record.untracked_us,
        record.terrain_collider_pending,
        record.contree_cache_pending,
        record.water_source_pending,
        record.water_collider_pending,
        record.water_cache_pending,
        record.ddgi_ready,
        record.visible_revision,
    );
}

fn log_gpu_frame(event_frame: u64, record: GpuFrameRecord) {
    let relative = record.source_frame as i64 - event_frame as i64;
    log::info!(
        "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=gpu_frame frame={} relative={} frame_render_us={:.0} tracer_render_us={:.0} scopes={} dropped={}",
        record.source_frame,
        relative,
        record.render_us,
        record.tracer_us,
        record.scopes,
        record.dropped,
    );
}

fn push_bounded<T>(records: &mut VecDeque<T>, record: T) {
    if records.len() == PRE_EVENT_FRAME_SAMPLES {
        records.pop_front();
    }
    records.push_back(record);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_job() -> BoundedTopologyJob {
        let bound = isolation_bound();
        let mut snapshot = vec![0; voxel_count(bound) as usize];
        let fixture = generate_hollow_canopy();
        for z in 0..FIXTURE_DIM.z {
            for y in 0..FIXTURE_DIM.y {
                for x in 0..FIXTURE_DIM.x {
                    let fixture_index = (x + FIXTURE_DIM.x * (y + FIXTURE_DIM.y * z)) as usize;
                    let world = FIXTURE_ORIGIN + UVec3::new(x, y, z);
                    let local = world - bound.min();
                    let index = (local.x
                        + bound.dimensions().x * (local.y + bound.dimensions().y * local.z))
                        as usize;
                    snapshot[index] = fixture[fixture_index];
                }
            }
        }
        BoundedTopologyJob::new(bound, snapshot, FIXTURE_ORIGIN).unwrap()
    }

    #[test]
    fn deterministic_fixture_has_exact_requested_size() {
        let voxels = generate_hollow_canopy();
        let solids = voxels
            .iter()
            .filter(|voxel| **voxel & VOXEL_TYPE_MASK as u8 != 0)
            .count();
        assert_eq!(solids, FIXTURE_VOXELS);
    }

    #[test]
    fn deterministic_sampling_never_changes_terrain_cardinality() {
        let component = (0..20_000)
            .map(|x| (UVec3::new(x, 2, 3), 1))
            .collect::<Vec<_>>();
        let visual = deterministic_visual_sample(&component, PARTICLE_CAPACITY);
        assert_eq!(component.len(), 20_000);
        assert_eq!(visual.len(), PARTICLE_CAPACITY);
        assert_eq!(deterministic_visual_sample(&component, 0).len(), 0);
    }

    #[test]
    fn oversized_bounded_trace_is_explicitly_pending_then_detached() {
        let mut job = fixture_job();
        let first = job.advance(PARTICLE_CAPACITY);
        assert_eq!(first.disposition, BoundedDisposition::Pending);
        assert_eq!(first.processed, PARTICLE_CAPACITY);
        assert_eq!(job.component_len(), PARTICLE_CAPACITY);
        while job.advance(PARTICLE_CAPACITY).disposition == BoundedDisposition::Pending {}
        assert_eq!(job.terminal, Some(BoundedDisposition::Detached));
        assert_eq!(job.component_len(), FIXTURE_VOXELS);
        assert_eq!(
            job.snapshot
                .iter()
                .filter(|voxel| **voxel & VOXEL_TYPE_MASK as u8 != 0)
                .count(),
            FIXTURE_VOXELS
        );
    }

    #[test]
    fn boundary_uncertainty_is_deferred_not_anchored_or_detached() {
        let bound = UAabb3::new(UVec3::new(10, 10, 10), UVec3::new(13, 13, 13));
        let mut snapshot = vec![0; voxel_count(bound) as usize];
        snapshot[13] = 1;
        snapshot[12] = 1;
        let mut job = BoundedTopologyJob::new(bound, snapshot, UVec3::new(11, 11, 11)).unwrap();
        assert_eq!(job.advance(10).disposition, BoundedDisposition::Deferred);
    }

    #[test]
    fn same_scale_component_reaching_world_floor_is_anchored() {
        let bound = UAabb3::new(
            UVec3::new(FIXTURE_ORIGIN.x - 1, 0, FIXTURE_ORIGIN.z - 1),
            FIXTURE_ORIGIN + FIXTURE_DIM + UVec3::ONE,
        );
        let dim = bound.dimensions();
        let mut snapshot = vec![0; voxel_count(bound) as usize];
        let index = |world: UVec3| {
            let local = world - bound.min();
            (local.x + dim.x * (local.y + dim.y * local.z)) as usize
        };
        let fixture = generate_hollow_canopy();
        for z in 0..FIXTURE_DIM.z {
            for y in 0..FIXTURE_DIM.y {
                for x in 0..FIXTURE_DIM.x {
                    let fixture_index = (x + FIXTURE_DIM.x * (y + FIXTURE_DIM.y * z)) as usize;
                    snapshot[index(FIXTURE_ORIGIN + UVec3::new(x, y, z))] = fixture[fixture_index];
                }
            }
        }
        for y in 0..FIXTURE_ORIGIN.y {
            snapshot[index(UVec3::new(FIXTURE_ORIGIN.x, y, FIXTURE_ORIGIN.z))] = 1;
        }
        assert!(
            snapshot
                .iter()
                .filter(|voxel| **voxel & VOXEL_TYPE_MASK as u8 != 0)
                .count()
                >= FIXTURE_VOXELS
        );
        let mut job = BoundedTopologyJob::new(bound, snapshot, FIXTURE_ORIGIN).unwrap();
        while job.advance(PARTICLE_CAPACITY).disposition == BoundedDisposition::Pending {}
        assert_eq!(job.terminal, Some(BoundedDisposition::Anchored));
    }
}
