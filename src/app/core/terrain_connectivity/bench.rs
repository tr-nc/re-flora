//! PROTOTYPE: deterministic release-mode workload for the oversized detached-terrain policy.
//!
//! Question: can the current atlas/publication architecture classify and atomically invalidate one
//! 437,205-voxel detached hollow canopy, then represent at most 16,384 voxels as particles, without
//! exceeding interactive frame budgets? This module is activated only by the diagnostic CLI.

use super::super::launch_owners::ScenarioOwner;
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
const MANUAL_SUPPORT_HALF_WIDTH: u32 = 2;
const VOXELS_PER_WORLD_UNIT: f32 = 256.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum BenchState {
    InstallFixture,
    AwaitingInstallResult {
        frame: u64,
    },
    Warmup {
        ready_after_frame: u64,
    },
    AwaitingManualEdit,
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
    AwaitingSnapshotResult {
        frame: u64,
        revision_before: u32,
    },
    AwaitingReleaseResult {
        event_frame: u64,
    },
    AwaitingAtomicityResult {
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
    AwaitingCommitResult {
        event_frame: u64,
    },
    Observing {
        event_frame: u64,
    },
    AwaitingVisualSpawnResult {
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

#[derive(Clone, Copy)]
struct ConnectivityFacts {
    frame: u64,
    visible_revision: u32,
    contree_idle: bool,
    terrain_collider_pending: usize,
    water_ready: bool,
    ddgi_ready: bool,
    available_particles: usize,
}

enum ConnectivityAction {
    None,
    InstallFixture {
        manual: bool,
        available_particles: usize,
    },
    ReadBoundedSnapshot {
        bound: UAabb3,
        seed: UVec3,
    },
    RunReleaseEvent {
        mode: TerrainConnectivityBenchMode,
    },
    ValidateAtomicity {
        bound: UAabb3,
        component: Vec<u32>,
        expected_available_particles: usize,
    },
    CommitBounded(BoundedCommitPayload),
    SpawnVisualVoxels {
        voxels: Vec<(UVec3, u8)>,
    },
}

struct BoundedCommitPayload {
    job: BoundedTopologyJob,
    release_frame: u64,
    revision_before: u32,
    snapshot_readback_us: f64,
    classification_us: f64,
    atomic_validation_us: f64,
    sampling_us: f64,
    staging_clear_us: f64,
    sampled_voxels: usize,
    manual: bool,
}

enum ConnectivityResult {
    None,
    FixtureInstalled(anyhow::Result<FixtureInstallResult>),
    SnapshotRead(anyhow::Result<SnapshotReadResult>),
    ReleaseEventRun(anyhow::Result<EventStages>),
    AtomicityValidated(anyhow::Result<AtomicityValidationResult>),
    BoundedCommitted(anyhow::Result<EventStages>),
    VisualVoxelsSpawned(anyhow::Result<VisualSpawnResult>),
}

struct FixtureInstallResult {
    setup_us: f64,
    reserve_us: f64,
    available_particles: usize,
    visible_revision: u32,
}

struct SnapshotReadResult {
    job: BoundedTopologyJob,
    snapshot_readback_us: f64,
}

struct AtomicityValidationResult {
    remaining_solids: usize,
    available_particles: usize,
    atomic_validation_us: f64,
}

struct VisualSpawnResult {
    requested_particles: usize,
    spawned_particles: usize,
    particle_spawn_us: f64,
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

    pub(in crate::app::core) fn try_begin_manual_release(app: &mut App) -> anyhow::Result<bool> {
        let frame = app.time_info.total_frame_count();
        let revision_before = app.visible_terrain_revision;
        let App {
            scenario_owner,
            terrain_connectivity,
            plain_builder,
            ..
        } = app;
        match scenario_owner {
            ScenarioOwner::TerrainConnectivityBenchmark(bench)
                if bench.options.mode == TerrainConnectivityBenchMode::Manual =>
            {
                bench
                    .begin_manual_release(
                        terrain_connectivity,
                        plain_builder,
                        frame,
                        revision_before,
                    )
                    .map(|_| true)
            }
            ScenarioOwner::Garden
            | ScenarioOwner::CanopyAudioDiagnostic(_)
            | ScenarioOwner::WaterExperience(_)
            | ScenarioOwner::WaterEditSoak(_)
            | ScenarioOwner::EnvironmentLighting(_)
            | ScenarioOwner::HybridTransparency(_)
            | ScenarioOwner::House(_)
            | ScenarioOwner::TerrainConnectivityBenchmark(_)
            | ScenarioOwner::FoliageShadowBenchmark(_) => Ok(false),
        }
    }

    fn begin_manual_release(
        &mut self,
        terrain_connectivity: &mut TerrainConnectivityRuntime,
        plain_builder: &mut PlainBuilder,
        frame: u64,
        revision_before: u32,
    ) -> anyhow::Result<()> {
        let world_dim = CHUNK_DIM * VOXEL_DIM_PER_CHUNK;
        if terrain_connectivity
            .take_player_release(world_dim)
            .is_none()
        {
            return Ok(());
        }
        if self.state != BenchState::AwaitingManualEdit {
            log::warn!(
                "[TERRAIN_CONNECTIVITY_MANUAL] ignored edit release while state={:?}",
                self.state
            );
            return Ok(());
        }

        let snapshot_started = Instant::now();
        let bound = isolation_bound();
        let snapshot = plain_builder.read_chunk_atlas_region(bound.min(), bound.dimensions())?;
        let snapshot_readback_us = snapshot_started.elapsed().as_secs_f64() * 1_000_000.0;
        self.bounded_job = Some(BoundedTopologyJob::new(
            bound,
            snapshot,
            manual_canopy_seed(),
        )?);
        self.state = BenchState::Tracing {
            release_frame: frame,
            revision_before,
            snapshot_readback_us,
            classification_us: 0.0,
        };
        log::info!(
            "[TERRAIN_CONNECTIVITY_MANUAL] phase=job_start release_frame={} revision={} voxel_budget={}",
            frame,
            revision_before,
            self.options.voxel_budget,
        );
        Ok(())
    }

    pub(in crate::app::core) fn advance(app: &mut App) -> anyhow::Result<()> {
        let water_ready = app.water_terrain_status().is_ready();
        let facts = ConnectivityFacts {
            frame: app.time_info.total_frame_count(),
            visible_revision: app.visible_terrain_revision,
            contree_idle: app.contree_builder.cpu_chunk_cache_jobs_idle(),
            terrain_collider_pending: app.terrain_physics.terrain_collider_pending_len(),
            water_ready,
            ddgi_ready: app
                .tracer
                .ddgi_ready_for_terrain_revision(app.visible_terrain_revision),
            available_particles: app.particle_system.available_capacity(),
        };
        let action = match &mut app.scenario_owner {
            ScenarioOwner::TerrainConnectivityBenchmark(bench) => bench.next_action(facts)?,
            ScenarioOwner::Garden
            | ScenarioOwner::CanopyAudioDiagnostic(_)
            | ScenarioOwner::WaterExperience(_)
            | ScenarioOwner::WaterEditSoak(_)
            | ScenarioOwner::EnvironmentLighting(_)
            | ScenarioOwner::HybridTransparency(_)
            | ScenarioOwner::House(_)
            | ScenarioOwner::FoliageShadowBenchmark(_) => ConnectivityAction::None,
        };
        let result = Self::execute_action(app, action);
        match &mut app.scenario_owner {
            ScenarioOwner::TerrainConnectivityBenchmark(bench) => bench.apply_result(result),
            ScenarioOwner::Garden
            | ScenarioOwner::CanopyAudioDiagnostic(_)
            | ScenarioOwner::WaterExperience(_)
            | ScenarioOwner::WaterEditSoak(_)
            | ScenarioOwner::EnvironmentLighting(_)
            | ScenarioOwner::HybridTransparency(_)
            | ScenarioOwner::House(_)
            | ScenarioOwner::FoliageShadowBenchmark(_) => match result {
                ConnectivityResult::None => Ok(()),
                ConnectivityResult::FixtureInstalled(_)
                | ConnectivityResult::SnapshotRead(_)
                | ConnectivityResult::ReleaseEventRun(_)
                | ConnectivityResult::AtomicityValidated(_)
                | ConnectivityResult::BoundedCommitted(_)
                | ConnectivityResult::VisualVoxelsSpawned(_) => {
                    anyhow::bail!("inactive scenario received a connectivity result")
                }
            },
        }
    }

    fn next_action(&mut self, facts: ConnectivityFacts) -> anyhow::Result<ConnectivityAction> {
        let frame = facts.frame;
        match self.state {
            BenchState::InstallFixture => {
                self.state = BenchState::AwaitingInstallResult { frame };
                return Ok(ConnectivityAction::InstallFixture {
                    manual: self.options.mode == TerrainConnectivityBenchMode::Manual,
                    available_particles: self.options.available_particles,
                });
            }
            BenchState::Warmup { ready_after_frame } => {
                let ready = frame >= ready_after_frame
                    && facts.contree_idle
                    && (self.options.mode == TerrainConnectivityBenchMode::Manual
                        || facts.terrain_collider_pending == 0)
                    && facts.water_ready
                    && facts.ddgi_ready;
                if ready {
                    anyhow::ensure!(
                        facts.available_particles == self.options.available_particles,
                        "terrain connectivity benchmark particle availability drifted: actual={} expected={}",
                        facts.available_particles,
                        self.options.available_particles,
                    );
                    if self.options.mode == TerrainConnectivityBenchMode::Manual {
                        self.state = BenchState::AwaitingManualEdit;
                        log::info!(
                            "[TERRAIN_CONNECTIVITY_MANUAL] phase=ready instruction=dig_through_the_sand_support_then_release_the_mouse revision={}",
                            facts.visible_revision,
                        );
                    } else if self.options.mode == TerrainConnectivityBenchMode::Bounded {
                        let bound = isolation_bound();
                        self.state = BenchState::AwaitingSnapshotResult {
                            frame,
                            revision_before: facts.visible_revision,
                        };
                        return Ok(ConnectivityAction::ReadBoundedSnapshot {
                            bound,
                            seed: FIXTURE_ORIGIN,
                        });
                    } else {
                        self.state = BenchState::AwaitingReleaseResult { event_frame: frame };
                        return Ok(ConnectivityAction::RunReleaseEvent {
                            mode: self.options.mode,
                        });
                    }
                } else if frame.is_multiple_of(120) {
                    log::info!(
                        "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=warmup frame={} ready_after={} contree_idle={} terrain_collider_pending={} water_ready={} ddgi_ready={} revision={}",
                        frame,
                        ready_after_frame,
                        facts.contree_idle,
                        facts.terrain_collider_pending,
                        facts.water_ready,
                        facts.ddgi_ready,
                        facts.visible_revision,
                    );
                }
            }
            BenchState::AwaitingManualEdit => {}
            BenchState::Tracing {
                release_frame,
                revision_before,
                snapshot_readback_us,
                mut classification_us,
            } => {
                anyhow::ensure!(
                    facts.visible_revision == revision_before,
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
                    facts.visible_revision,
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
                        if self.options.mode == TerrainConnectivityBenchMode::Manual {
                            log::info!(
                                "[TERRAIN_CONNECTIVITY_MANUAL] phase=still_connected disposition={} processed_voxels={} revision={}",
                                step.disposition.label(),
                                job.component_len(),
                                facts.visible_revision,
                            );
                            self.bounded_job = None;
                            self.state = BenchState::AwaitingManualEdit;
                        } else {
                            anyhow::bail!(
                                "bounded detached fixture ended as {}",
                                step.disposition.label()
                            );
                        }
                    }
                }
            }
            BenchState::ValidateAtomicity {
                release_frame,
                revision_before,
                snapshot_readback_us,
                classification_us,
            } => {
                let job = self
                    .bounded_job
                    .as_ref()
                    .context("bounded topology validation lost its job")?;
                anyhow::ensure!(facts.visible_revision == revision_before);
                let action = ConnectivityAction::ValidateAtomicity {
                    bound: job.bound,
                    component: job.component.clone(),
                    expected_available_particles: self.options.available_particles,
                };
                self.state = BenchState::AwaitingAtomicityResult {
                    release_frame,
                    revision_before,
                    snapshot_readback_us,
                    classification_us,
                };
                return Ok(action);
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
                let payload = BoundedCommitPayload {
                    job: self
                        .bounded_job
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
                    manual: self.options.mode == TerrainConnectivityBenchMode::Manual,
                };
                self.state = BenchState::AwaitingCommitResult { event_frame };
                return Ok(ConnectivityAction::CommitBounded(payload));
            }
            BenchState::Observing { event_frame } => {
                if let Some(visual_voxels) = self.pending_visual_voxels.take() {
                    self.state = BenchState::AwaitingVisualSpawnResult { event_frame };
                    return Ok(ConnectivityAction::SpawnVisualVoxels {
                        voxels: visual_voxels,
                    });
                }
            }
            BenchState::AwaitingInstallResult { .. }
            | BenchState::AwaitingSnapshotResult { .. }
            | BenchState::AwaitingReleaseResult { .. }
            | BenchState::AwaitingAtomicityResult { .. }
            | BenchState::AwaitingCommitResult { .. }
            | BenchState::AwaitingVisualSpawnResult { .. } => {
                anyhow::bail!("connectivity bench advanced while awaiting an action result")
            }
            BenchState::Complete => {}
        }
        Ok(ConnectivityAction::None)
    }

    fn execute_action(app: &mut App, action: ConnectivityAction) -> ConnectivityResult {
        match action {
            ConnectivityAction::None => ConnectivityResult::None,
            ConnectivityAction::InstallFixture {
                manual,
                available_particles,
            } => ConnectivityResult::FixtureInstalled((|| {
                let started = Instant::now();
                if manual {
                    install_manual_fixture(app)?;
                    configure_manual_camera(app)?;
                } else {
                    install_fixture(app)?;
                }
                let reserve_started = Instant::now();
                set_available_particle_capacity(app, available_particles)?;
                Ok(FixtureInstallResult {
                    setup_us: started.elapsed().as_secs_f64() * 1_000_000.0,
                    reserve_us: reserve_started.elapsed().as_secs_f64() * 1_000_000.0,
                    available_particles: app.particle_system.available_capacity(),
                    visible_revision: app.visible_terrain_revision,
                })
            })()),
            ConnectivityAction::ReadBoundedSnapshot { bound, seed } => {
                ConnectivityResult::SnapshotRead((|| {
                    let snapshot_started = Instant::now();
                    let snapshot = app
                        .plain_builder
                        .read_chunk_atlas_region(bound.min(), bound.dimensions())?;
                    Ok(SnapshotReadResult {
                        job: BoundedTopologyJob::new(bound, snapshot, seed)?,
                        snapshot_readback_us: snapshot_started.elapsed().as_secs_f64()
                            * 1_000_000.0,
                    })
                })())
            }
            ConnectivityAction::RunReleaseEvent { mode } => {
                ConnectivityResult::ReleaseEventRun(run_release_event(app, mode))
            }
            ConnectivityAction::ValidateAtomicity {
                bound,
                component,
                expected_available_particles,
            } => ConnectivityResult::AtomicityValidated((|| {
                let validation_started = Instant::now();
                let remaining_solids =
                    count_component_solids(&mut app.plain_builder, bound, &component)?;
                let atomic_validation_us = validation_started.elapsed().as_secs_f64() * 1_000_000.0;
                let available_particles = app.particle_system.available_capacity();
                anyhow::ensure!(available_particles == expected_available_particles);
                Ok(AtomicityValidationResult {
                    remaining_solids,
                    available_particles,
                    atomic_validation_us,
                })
            })()),
            ConnectivityAction::CommitBounded(payload) => {
                ConnectivityResult::BoundedCommitted(run_bounded_commit(
                    app,
                    payload.job,
                    payload.release_frame,
                    payload.revision_before,
                    payload.snapshot_readback_us,
                    payload.classification_us,
                    payload.atomic_validation_us,
                    payload.sampling_us,
                    payload.staging_clear_us,
                    payload.sampled_voxels,
                    payload.manual,
                ))
            }
            ConnectivityAction::SpawnVisualVoxels { voxels } => {
                ConnectivityResult::VisualVoxelsSpawned((|| {
                    let particle_started = Instant::now();
                    let requested_particles = voxels.len();
                    let spawned_particles = app.spawn_detached_terrain_voxel_particles(&voxels);
                    anyhow::ensure!(spawned_particles == requested_particles);
                    Ok(VisualSpawnResult {
                        requested_particles,
                        spawned_particles,
                        particle_spawn_us: particle_started.elapsed().as_secs_f64() * 1_000_000.0,
                    })
                })())
            }
        }
    }

    fn apply_result(&mut self, result: ConnectivityResult) -> anyhow::Result<()> {
        match (self.state, result) {
            (_, ConnectivityResult::None) => Ok(()),
            (
                BenchState::AwaitingInstallResult { frame },
                ConnectivityResult::FixtureInstalled(outcome),
            ) => {
                let installed = outcome?;
                let ready_after_frame = frame.saturating_add(u64::from(self.options.warmup_frames));
                self.state = BenchState::Warmup { ready_after_frame };
                log::info!(
                    "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=fixture mode={} frame={} fixture_voxels={} fixture_min={:?} fixture_max={:?} affected_chunks=4 available_particles={} reserve_us={:.0} setup_us={:.0} warmup_frames={} observe_frames={} revision={}",
                    self.options.mode.label(),
                    frame,
                    FIXTURE_VOXELS,
                    fixture_bound().min(),
                    fixture_bound().max(),
                    installed.available_particles,
                    installed.reserve_us,
                    installed.setup_us,
                    self.options.warmup_frames,
                    self.options.observe_frames,
                    installed.visible_revision,
                );
                Ok(())
            }
            (
                BenchState::AwaitingSnapshotResult {
                    frame,
                    revision_before,
                },
                ConnectivityResult::SnapshotRead(outcome),
            ) => {
                let snapshot = outcome?;
                let bound = snapshot.job.bound;
                self.bounded_job = Some(snapshot.job);
                self.state = BenchState::Tracing {
                    release_frame: frame,
                    revision_before,
                    snapshot_readback_us: snapshot.snapshot_readback_us,
                    classification_us: 0.0,
                };
                log::info!(
                    "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=job_start mode=bounded release_frame={} revision={} snapshot_voxels={} snapshot_readback_us={:.0} voxel_budget={} available_particles={}",
                    frame,
                    revision_before,
                    voxel_count(bound),
                    snapshot.snapshot_readback_us,
                    self.options.voxel_budget,
                    self.options.available_particles,
                );
                Ok(())
            }
            (
                BenchState::AwaitingReleaseResult { event_frame },
                ConnectivityResult::ReleaseEventRun(outcome),
            ) => {
                let stages = outcome?;
                self.stages = Some(stages);
                self.state = BenchState::Observing { event_frame };
                log_event(self.options, event_frame, stages);
                Ok(())
            }
            (
                BenchState::AwaitingAtomicityResult {
                    release_frame,
                    revision_before,
                    snapshot_readback_us,
                    classification_us,
                },
                ConnectivityResult::AtomicityValidated(outcome),
            ) => {
                let validation = outcome?;
                let job = self
                    .bounded_job
                    .as_mut()
                    .context("bounded topology validation lost its job")?;
                anyhow::ensure!(
                    validation.remaining_solids == job.component_len(),
                    "bounded topology modified live terrain while pending: remaining={} expected={}",
                    validation.remaining_solids,
                    job.component_len(),
                );
                log::info!(
                    "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=atomic_check mode=bounded release_frame={} remaining_fixture_voxels={} visible_revision={} validation_us={:.0} available_particles={}",
                    release_frame,
                    validation.remaining_solids,
                    revision_before,
                    validation.atomic_validation_us,
                    validation.available_particles,
                );
                let (visual_voxels, sampling_us, staging_clear_us) =
                    prepare_bounded_commit(job, self.options.available_particles);
                let sampled_voxels = visual_voxels.len();
                self.pending_visual_voxels = Some(visual_voxels);
                self.state = BenchState::Commit {
                    release_frame,
                    revision_before,
                    snapshot_readback_us,
                    classification_us,
                    atomic_validation_us: validation.atomic_validation_us,
                    sampling_us,
                    staging_clear_us,
                    sampled_voxels,
                };
                Ok(())
            }
            (
                BenchState::AwaitingCommitResult { event_frame },
                ConnectivityResult::BoundedCommitted(outcome),
            ) => {
                let stages = outcome?;
                self.stages = Some(stages);
                self.state = BenchState::Observing { event_frame };
                log_event(self.options, event_frame, stages);
                Ok(())
            }
            (
                BenchState::AwaitingVisualSpawnResult { event_frame },
                ConnectivityResult::VisualVoxelsSpawned(outcome),
            ) => {
                let visual = outcome?;
                anyhow::ensure!(visual.spawned_particles == visual.requested_particles);
                let stages = self
                    .stages
                    .as_mut()
                    .context("bounded visual spawn lost event stages")?;
                stages.particle_spawn_us = visual.particle_spawn_us;
                stages.spawned_particles = visual.spawned_particles;
                self.state = BenchState::Observing { event_frame };
                log::info!(
                    "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=visual_spawn mode=bounded event_frame={} spawned_particles={} particle_spawn_us={:.0}",
                    event_frame,
                    visual.spawned_particles,
                    visual.particle_spawn_us,
                );
                Ok(())
            }
            (state, _) => {
                anyhow::bail!("connectivity action result does not match bench state {state:?}")
            }
        }
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
            | BenchState::AwaitingInstallResult { .. }
            | BenchState::Warmup { .. }
            | BenchState::AwaitingManualEdit
            | BenchState::Tracing { .. }
            | BenchState::ValidateAtomicity { .. }
            | BenchState::AwaitingSnapshotResult { .. }
            | BenchState::AwaitingReleaseResult { .. }
            | BenchState::AwaitingAtomicityResult { .. }
            | BenchState::Commit { .. }
            | BenchState::AwaitingCommitResult { .. }
            | BenchState::AwaitingVisualSpawnResult { .. } => {
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
        let App {
            scenario_owner,
            plain_builder,
            ..
        } = app;
        match scenario_owner {
            ScenarioOwner::TerrainConnectivityBenchmark(bench) => {
                bench.observe_completed_frame_inner(plain_builder, record)
            }
            ScenarioOwner::Garden
            | ScenarioOwner::CanopyAudioDiagnostic(_)
            | ScenarioOwner::WaterExperience(_)
            | ScenarioOwner::WaterEditSoak(_)
            | ScenarioOwner::EnvironmentLighting(_)
            | ScenarioOwner::HybridTransparency(_)
            | ScenarioOwner::House(_)
            | ScenarioOwner::FoliageShadowBenchmark(_) => Ok(false),
        }
    }

    fn observe_completed_frame_inner(
        &mut self,
        plain_builder: &mut PlainBuilder,
        record: CpuFrameRecord,
    ) -> anyhow::Result<bool> {
        match self.state {
            BenchState::InstallFixture
            | BenchState::AwaitingInstallResult { .. }
            | BenchState::Warmup { .. }
            | BenchState::AwaitingManualEdit
            | BenchState::Tracing { .. }
            | BenchState::ValidateAtomicity { .. }
            | BenchState::AwaitingSnapshotResult { .. }
            | BenchState::AwaitingReleaseResult { .. }
            | BenchState::AwaitingAtomicityResult { .. }
            | BenchState::Commit { .. }
            | BenchState::AwaitingCommitResult { .. }
            | BenchState::AwaitingVisualSpawnResult { .. } => {
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
                    let remaining = count_fixture_solids(plain_builder)?;
                    let expected = match self.options.mode {
                        TerrainConnectivityBenchMode::Existing => FIXTURE_VOXELS,
                        TerrainConnectivityBenchMode::Correct
                        | TerrainConnectivityBenchMode::Bounded
                        | TerrainConnectivityBenchMode::Manual => 0,
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
                    return Ok(self.options.mode != TerrainConnectivityBenchMode::Manual);
                }
                Ok(false)
            }
            BenchState::Complete => Ok(self.options.mode != TerrainConnectivityBenchMode::Manual),
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
    app.terrain_connectivity
        .observe_player_publication(fixture_edit_bound(), true);

    if mode == TerrainConnectivityBenchMode::Existing {
        let current_started = Instant::now();
        app.finish_player_terrain_connectivity_hold()?;
        return Ok(EventStages {
            total_us: total_started.elapsed().as_secs_f64() * 1_000_000.0,
            current_path_us: current_started.elapsed().as_secs_f64() * 1_000_000.0,
            revision_before,
            revision_after: app.visible_terrain_revision,
            ..EventStages::default()
        });
    }

    let world_dim = CHUNK_DIM * VOXEL_DIM_PER_CHUNK;
    let TerrainConnectivityRequest::PlayerEdit { edited, block } = app
        .terrain_connectivity
        .take_player_release(world_dim)
        .context("bench edit region disappeared")?
    else {
        anyhow::bail!("bench expected a player-edit connectivity request");
    };
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
    for (origin, dim, data) in
        prepare_detached_voxel_clear(&mut app.plain_builder, world_dim, &component.voxels)?
    {
        app.plain_builder
            .write_chunk_atlas_region(origin, dim, &data)?;
    }
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
    manual: bool,
) -> anyhow::Result<EventStages> {
    let total_started = Instant::now();
    anyhow::ensure!(
        app.visible_terrain_revision == revision_before,
        "bounded topology commit revision changed from {} to {}",
        revision_before,
        app.visible_terrain_revision,
    );
    anyhow::ensure!(job.terminal == Some(BoundedDisposition::Detached));
    if !manual {
        anyhow::ensure!(
            job.component.len() == FIXTURE_VOXELS,
            "bounded topology commit is not one complete detached fixture"
        );
    }
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
    anyhow::ensure!(count_fixture_solids(&mut app.plain_builder)? == FIXTURE_VOXELS);
    Ok(())
}

fn install_manual_fixture(app: &mut App) -> anyhow::Result<()> {
    install_fixture(app)?;
    let bound = manual_support_bound();
    let support = generate_manual_support();
    app.plain_builder
        .write_chunk_atlas_region(bound.min(), bound.dimensions(), &support)?;
    app.plain_builder.mark_all_solid_workgroups_dirty();
    let change =
        VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(bound)])?
            .context("manual support installation has no visible terrain chunks")?;
    app.publish_visible_terrain(change)?;
    Ok(())
}

fn configure_manual_camera(app: &mut App) -> anyhow::Result<()> {
    let center = manual_support_center().as_vec3() / VOXELS_PER_WORLD_UNIT;
    let target = center + Vec3::Y * 0.035;
    let position = target + Vec3::new(-0.24, 0.04, -0.28);
    app.camera_control.apply_snapshot_mode(true);
    app.camera_control.set_orbit_focus(target);
    anyhow::ensure!(
        app.tracer.set_camera_pose_looking_at(position, target),
        "failed to configure terrain connectivity manual camera"
    );
    Ok(())
}

fn manual_support_center() -> UVec3 {
    UVec3::new(
        FIXTURE_ORIGIN.x + FIXTURE_DIM.x / 2,
        FIXTURE_ORIGIN.y,
        FIXTURE_ORIGIN.z + FIXTURE_DIM.z / 2,
    )
}

fn manual_support_bound() -> UAabb3 {
    let center = manual_support_center();
    let min = UVec3::new(
        center.x - MANUAL_SUPPORT_HALF_WIDTH,
        0,
        center.z - MANUAL_SUPPORT_HALF_WIDTH,
    );
    let max = UVec3::new(
        center.x + MANUAL_SUPPORT_HALF_WIDTH + 1,
        FIXTURE_ORIGIN.y + FIXTURE_DIM.y,
        center.z + MANUAL_SUPPORT_HALF_WIDTH + 1,
    );
    UAabb3::new(min, max)
}

fn generate_manual_support() -> Vec<u8> {
    let bound = manual_support_bound();
    vec![crate::builder::VOXEL_TYPE_SAND as u8; voxel_count(bound) as usize]
}

fn manual_canopy_seed() -> UVec3 {
    FIXTURE_ORIGIN
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

fn count_fixture_solids(plain_builder: &mut PlainBuilder) -> anyhow::Result<usize> {
    let bound = fixture_bound();
    let voxels = plain_builder.read_chunk_atlas_region(bound.min(), bound.dimensions())?;
    Ok(voxels
        .iter()
        .filter(|voxel| **voxel & VOXEL_TYPE_MASK as u8 != 0)
        .count())
}

fn count_component_solids(
    plain_builder: &mut PlainBuilder,
    bound: UAabb3,
    component: &[u32],
) -> anyhow::Result<usize> {
    let voxels = plain_builder.read_chunk_atlas_region(bound.min(), bound.dimensions())?;
    Ok(component
        .iter()
        .filter(|index| voxels[**index as usize] & VOXEL_TYPE_MASK as u8 != 0)
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

    fn manual_fixture_job(cut_support_at_y: Option<u32>) -> BoundedTopologyJob {
        let mut job = fixture_job();
        let support = manual_support_bound();
        for z in support.min().z..support.max().z {
            for y in isolation_bound().min().y..support.max().y {
                for x in support.min().x..support.max().x {
                    if cut_support_at_y == Some(y) {
                        continue;
                    }
                    let local = UVec3::new(x, y, z) - job.bound.min();
                    let index = job.index_of(local);
                    job.snapshot[index as usize] = crate::builder::VOXEL_TYPE_SAND as u8;
                }
            }
        }
        job
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
    fn planning_an_action_keeps_the_scenario_identity_and_records_the_awaited_result() {
        let options = TerrainConnectivityBenchOptions {
            mode: TerrainConnectivityBenchMode::Bounded,
            available_particles: 8,
            warmup_frames: 1,
            observe_frames: 1,
            voxel_budget: 8,
        };
        let mut owner =
            ScenarioOwner::TerrainConnectivityBenchmark(TerrainConnectivityBench::new(options));
        let action = match &mut owner {
            ScenarioOwner::TerrainConnectivityBenchmark(bench) => bench
                .next_action(ConnectivityFacts {
                    frame: 17,
                    visible_revision: 4,
                    contree_idle: true,
                    terrain_collider_pending: 0,
                    water_ready: true,
                    ddgi_ready: true,
                    available_particles: 8,
                })
                .unwrap(),
            ScenarioOwner::Garden
            | ScenarioOwner::CanopyAudioDiagnostic(_)
            | ScenarioOwner::WaterExperience(_)
            | ScenarioOwner::WaterEditSoak(_)
            | ScenarioOwner::EnvironmentLighting(_)
            | ScenarioOwner::HybridTransparency(_)
            | ScenarioOwner::House(_)
            | ScenarioOwner::FoliageShadowBenchmark(_) => {
                panic!("test constructed the wrong scenario")
            }
        };

        assert!(matches!(
            action,
            ConnectivityAction::InstallFixture {
                manual: false,
                available_particles: 8,
            }
        ));
        assert!(matches!(
            owner,
            ScenarioOwner::TerrainConnectivityBenchmark(TerrainConnectivityBench {
                state: BenchState::AwaitingInstallResult { frame: 17 },
                ..
            })
        ));
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
    fn manual_canopy_stays_connected_until_the_support_is_cut() {
        let mut connected = manual_fixture_job(None);
        while connected.advance(PARTICLE_CAPACITY).disposition == BoundedDisposition::Pending {}
        assert_eq!(connected.terminal, Some(BoundedDisposition::Deferred));

        let mut detached = manual_fixture_job(Some(FIXTURE_ORIGIN.y + 4));
        while detached.advance(PARTICLE_CAPACITY).disposition == BoundedDisposition::Pending {}
        assert_eq!(detached.terminal, Some(BoundedDisposition::Detached));
        assert!(detached.component_len() >= FIXTURE_VOXELS);
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
