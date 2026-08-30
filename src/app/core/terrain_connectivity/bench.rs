//! PROTOTYPE: deterministic release-mode workload for the oversized detached-terrain policy.
//!
//! Question: can the current atlas/publication architecture classify and atomically invalidate one
//! 437,205-voxel detached hollow canopy, then represent at most 16,384 voxels as particles, without
//! exceeding interactive frame budgets? This module is activated only by the diagnostic CLI.

use super::super::launch_owners::ScenarioOwner;
use super::detachment::{PreparedTerrainDetachment, TerrainDetachmentRequest};
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

#[derive(Debug)]
enum BenchState {
    InstallFixture,
    Warmup {
        ready_after_frame: u64,
    },
    AwaitingManualEdit,
    RetryManualRelease {
        request: ManualReleaseRequest,
        resume: Box<BenchState>,
    },
    Tracing {
        job: BoundedTopologyJob,
        release_frame: u64,
        revision_before: u32,
        snapshot_readback_us: f64,
        classification_us: f64,
    },
    ValidateAtomicity {
        job: BoundedTopologyJob,
        release_frame: u64,
        revision_before: u32,
        snapshot_readback_us: f64,
        classification_us: f64,
    },
    AwaitingSnapshotResult {
        frame: u64,
        revision_before: u32,
        ready_after_frame: u64,
    },
    AwaitingReleaseResult {
        event_frame: u64,
        ready_after_frame: u64,
    },
    AwaitingAtomicityResult {
        job: BoundedTopologyJob,
        release_frame: u64,
        revision_before: u32,
        snapshot_readback_us: f64,
        classification_us: f64,
    },
    Commit(BoundedCommitPayload),
    AwaitingCommitResult {
        event_frame: u64,
    },
    Observing {
        event_frame: u64,
    },
    RetryCompletedFrame {
        request: CompletedFrameRequest,
        resume: Box<BenchState>,
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

#[derive(Debug)]
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

#[derive(Clone, Copy)]
struct ManualReleaseFacts {
    frame: u64,
    visible_revision: u32,
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
    HandleManualRelease(ManualReleaseRequest),
    ObserveCompletedFrame(CompletedFrameRequest),
}

struct FixtureInstallRequest {
    manual: bool,
    available_particles: usize,
}

struct SnapshotReadRequest {
    bound: UAabb3,
    seed: UVec3,
}

struct ReleaseEventRequest {
    mode: TerrainConnectivityBenchMode,
}

struct AtomicityValidationRequest {
    bound: UAabb3,
    component: Vec<u32>,
    expected_available_particles: usize,
}

#[derive(Debug)]
struct CompletedFramePayload {
    record: CpuFrameRecord,
    expected_fixture_solids: Option<usize>,
}

#[derive(Debug)]
struct CompletedFrameRequest(Box<CompletedFramePayload>);

impl CompletedFrameRequest {
    fn new(record: CpuFrameRecord, expected_fixture_solids: Option<usize>) -> Self {
        Self(Box::new(CompletedFramePayload {
            record,
            expected_fixture_solids,
        }))
    }

    fn payload(&self) -> &CompletedFramePayload {
        &self.0
    }
}

struct FailedConnectivityAction<R> {
    request: R,
    error: anyhow::Error,
}

#[derive(Debug)]
enum ManualReleasePlan {
    Ignore,
    Prepare {
        frame: u64,
        revision_before: u32,
        bound: UAabb3,
        seed: UVec3,
    },
}

#[derive(Debug)]
struct ManualReleaseRequest(Box<ManualReleasePlan>);

impl ManualReleaseRequest {
    fn new(plan: ManualReleasePlan) -> Self {
        Self(Box::new(plan))
    }

    fn payload(&self) -> &ManualReleasePlan {
        &self.0
    }
}

#[derive(Debug)]
struct BoundedCommitPayload {
    job: BoundedTopologyJob,
    visual_voxels: Vec<(UVec3, u8)>,
    release_frame: u64,
    revision_before: u32,
    snapshot_readback_us: f64,
    classification_us: f64,
    atomic_validation_us: f64,
    sampling_us: f64,
    staging_clear_us: f64,
    manual: bool,
}

enum ConnectivityResult {
    None,
    FixtureInstalled {
        frame: u64,
        outcome: Result<FixtureInstallResult, FailedConnectivityAction<FixtureInstallRequest>>,
    },
    SnapshotRead(Result<SnapshotReadResult, FailedConnectivityAction<SnapshotReadRequest>>),
    ReleaseEventRun(Result<EventStages, FailedConnectivityAction<ReleaseEventRequest>>),
    AtomicityValidated(
        Result<AtomicityValidationResult, FailedConnectivityAction<AtomicityValidationRequest>>,
    ),
    BoundedCommitted(Result<EventStages, FailedConnectivityAction<BoundedCommitPayload>>),
    ManualReleaseHandled(
        Result<Option<ManualReleasePrepared>, FailedConnectivityAction<ManualReleaseRequest>>,
    ),
    CompletedFrameObserved(
        Result<CompletedFrameObservation, FailedConnectivityAction<CompletedFrameRequest>>,
    ),
}

#[derive(Debug, Default)]
struct ConnectivityEffect {
    manual_release_handled: bool,
    observation_complete: bool,
}

mod app_executor {
    use super::*;

    pub(super) struct ConnectivityExecution(ConnectivityResult);

    impl ConnectivityExecution {
        pub(super) fn into_result(self) -> ConnectivityResult {
            self.0
        }
    }

    pub(super) fn execute(app: &mut App, action: ConnectivityAction) -> ConnectivityExecution {
        let result = match action {
            ConnectivityAction::None => ConnectivityResult::None,
            ConnectivityAction::InstallFixture {
                manual,
                available_particles,
            } => {
                let frame = app.time_info.total_frame_count();
                let request = FixtureInstallRequest {
                    manual,
                    available_particles,
                };
                ConnectivityResult::FixtureInstalled {
                    frame,
                    outcome: match app.prepare_connectivity_fixture_installation(request) {
                        Ok(prepared) => Ok(prepared.commit(app)),
                        Err(failure) => Err(failure),
                    },
                }
            }
            ConnectivityAction::ReadBoundedSnapshot { bound, seed } => {
                let request = SnapshotReadRequest { bound, seed };
                let outcome = (|| {
                    let snapshot_started = Instant::now();
                    let snapshot = app
                        .plain_builder
                        .read_chunk_atlas_region(request.bound.min(), request.bound.dimensions())?;
                    Ok(SnapshotReadResult {
                        job: BoundedTopologyJob::new(request.bound, snapshot, request.seed)?,
                        snapshot_readback_us: snapshot_started.elapsed().as_secs_f64()
                            * 1_000_000.0,
                    })
                })()
                .map_err(|error| FailedConnectivityAction { request, error });
                ConnectivityResult::SnapshotRead(outcome)
            }
            ConnectivityAction::RunReleaseEvent { mode } => {
                let request = ReleaseEventRequest { mode };
                let outcome = app
                    .run_connectivity_release_event(request.mode)
                    .map_err(|error| FailedConnectivityAction { request, error });
                ConnectivityResult::ReleaseEventRun(outcome)
            }
            ConnectivityAction::ValidateAtomicity {
                bound,
                component,
                expected_available_particles,
            } => {
                let request = AtomicityValidationRequest {
                    bound,
                    component,
                    expected_available_particles,
                };
                let outcome = (|| {
                    let validation_started = Instant::now();
                    let remaining_solids = count_component_solids(
                        &mut app.plain_builder,
                        request.bound,
                        &request.component,
                    )?;
                    let atomic_validation_us =
                        validation_started.elapsed().as_secs_f64() * 1_000_000.0;
                    let available_particles = app.particle_system.available_capacity();
                    anyhow::ensure!(available_particles == request.expected_available_particles);
                    Ok(AtomicityValidationResult {
                        remaining_solids,
                        available_particles,
                        atomic_validation_us,
                    })
                })()
                .map_err(|error| FailedConnectivityAction { request, error });
                ConnectivityResult::AtomicityValidated(outcome)
            }
            ConnectivityAction::CommitBounded(payload) => {
                let facts = BoundedPrepareFacts {
                    visible_revision: app.visible_terrain_revision,
                    available_particles: app.particle_system.available_capacity(),
                };
                let outcome = match PreparedBoundedConnectivity::prepare(payload, facts) {
                    Ok(prepared) => Ok(prepared.commit(app)),
                    Err(failure) => Err(failure),
                };
                ConnectivityResult::BoundedCommitted(outcome)
            }
            ConnectivityAction::HandleManualRelease(plan) => {
                let request = plan;
                let outcome = match request.payload() {
                    ManualReleasePlan::Ignore => Ok(None),
                    ManualReleasePlan::Prepare {
                        frame,
                        revision_before,
                        bound,
                        seed,
                    } => {
                        let world_dim = CHUNK_DIM * VOXEL_DIM_PER_CHUNK;
                        let App {
                            terrain_connectivity,
                            plain_builder,
                            ..
                        } = app;
                        terrain_connectivity.transact_player_release(world_dim, || {
                            let snapshot_started = Instant::now();
                            let snapshot = plain_builder
                                .read_chunk_atlas_region(bound.min(), bound.dimensions())?;
                            Ok(ManualReleasePrepared {
                                frame: *frame,
                                revision_before: *revision_before,
                                snapshot: SnapshotReadResult {
                                    job: BoundedTopologyJob::new(*bound, snapshot, *seed)?,
                                    snapshot_readback_us: snapshot_started.elapsed().as_secs_f64()
                                        * 1_000_000.0,
                                },
                            })
                        })
                    }
                }
                .map_err(|error| FailedConnectivityAction { request, error });
                ConnectivityResult::ManualReleaseHandled(outcome)
            }
            ConnectivityAction::ObserveCompletedFrame(request) => {
                let outcome = (|| {
                    let fixture_count = match request.payload().expected_fixture_solids {
                        Some(expected) => {
                            count_fixture_solids(&mut app.plain_builder).map(|remaining| {
                                Some(FixtureCount {
                                    remaining,
                                    expected,
                                })
                            })
                        }
                        None => Ok(None),
                    }?;
                    Ok(CompletedFrameObservation {
                        record: request.payload().record,
                        fixture_count,
                    })
                })()
                .map_err(|error| FailedConnectivityAction { request, error });
                ConnectivityResult::CompletedFrameObserved(outcome)
            }
        };
        ConnectivityExecution(result)
    }
}

use app_executor::ConnectivityExecution;

::static_assertions::assert_not_impl_any!(ConnectivityExecution: Clone, Copy);

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

struct ManualReleasePrepared {
    frame: u64,
    revision_before: u32,
    snapshot: SnapshotReadResult,
}

#[derive(Clone, Copy)]
struct FixtureCount {
    remaining: usize,
    expected: usize,
}

struct CompletedFrameObservation {
    record: CpuFrameRecord,
    fixture_count: Option<FixtureCount>,
}

struct AtomicityValidationResult {
    remaining_solids: usize,
    available_particles: usize,
    atomic_validation_us: f64,
}

impl ScenarioOwner {
    fn plan_connectivity_action(
        &mut self,
        facts: ConnectivityFacts,
    ) -> anyhow::Result<ConnectivityAction> {
        match self {
            Self::Connectivity(bench) => bench.next_action(facts),
            Self::Standard(_) => Ok(ConnectivityAction::None),
        }
    }

    fn apply_connectivity_execution(
        &mut self,
        execution: ConnectivityExecution,
    ) -> anyhow::Result<ConnectivityEffect> {
        let result = execution.into_result();
        let manual_release_handled = matches!(&result, ConnectivityResult::ManualReleaseHandled(_));
        let observation_requested =
            matches!(&result, ConnectivityResult::CompletedFrameObserved(_));
        match self {
            Self::Connectivity(bench) => {
                bench.apply_result(result)?;
                Ok(ConnectivityEffect {
                    manual_release_handled,
                    observation_complete: observation_requested
                        && matches!(bench.state, BenchState::Complete)
                        && bench.options.mode != TerrainConnectivityBenchMode::Manual,
                })
            }
            Self::Standard(_) => {
                ensure_inactive_connectivity_result(result)?;
                Ok(ConnectivityEffect::default())
            }
        }
    }

    pub(in crate::app::core) fn record_connectivity_gpu_submission(
        &mut self,
        frame_slot: usize,
        frame: u64,
    ) {
        match self {
            Self::Connectivity(bench) => bench.record_gpu_submission(frame_slot, frame),
            Self::Standard(_) => {}
        }
    }

    pub(in crate::app::core) fn observe_connectivity_gpu_completion(
        &mut self,
        frame_slot: usize,
        results: &GpuProfilerFrameResults,
    ) {
        match self {
            Self::Connectivity(bench) => bench.observe_gpu_completion(frame_slot, results),
            Self::Standard(_) => {}
        }
    }

    pub(in crate::app::core) fn allows_ambient_particle_emitters(&mut self) -> bool {
        match self {
            Self::Connectivity(bench) => matches!(bench.state, BenchState::Complete),
            Self::Standard(_) => true,
        }
    }

    fn plan_manual_connectivity_release(
        &mut self,
        facts: ManualReleaseFacts,
    ) -> anyhow::Result<ConnectivityAction> {
        match self {
            Self::Connectivity(bench)
                if bench.options.mode == TerrainConnectivityBenchMode::Manual =>
            {
                Ok(bench.plan_manual_release(facts))
            }
            Self::Connectivity(_) | Self::Standard(_) => Ok(ConnectivityAction::None),
        }
    }

    fn plan_completed_connectivity_frame(
        &mut self,
        record: CpuFrameRecord,
    ) -> anyhow::Result<ConnectivityAction> {
        match self {
            Self::Connectivity(bench) => Ok(bench.plan_completed_frame(record)),
            Self::Standard(_) => Ok(ConnectivityAction::None),
        }
    }
}

fn ensure_inactive_connectivity_result(result: ConnectivityResult) -> anyhow::Result<()> {
    match result {
        ConnectivityResult::None => Ok(()),
        ConnectivityResult::SnapshotRead(_)
        | ConnectivityResult::ReleaseEventRun(_)
        | ConnectivityResult::AtomicityValidated(_)
        | ConnectivityResult::BoundedCommitted(_)
        | ConnectivityResult::ManualReleaseHandled(_)
        | ConnectivityResult::CompletedFrameObserved(_)
        | ConnectivityResult::FixtureInstalled { .. } => {
            anyhow::bail!("inactive scenario received a connectivity result")
        }
    }
}

impl App {
    pub(in crate::app::core) fn try_begin_manual_connectivity_benchmark_release(
        &mut self,
    ) -> anyhow::Result<bool> {
        let facts = ManualReleaseFacts {
            frame: self.time_info.total_frame_count(),
            visible_revision: self.visible_terrain_revision,
        };
        let action = self
            .scenario_owner
            .plan_manual_connectivity_release(facts)?;
        let execution = self.execute_connectivity_action(action);
        let effect = self
            .scenario_owner
            .apply_connectivity_execution(execution)?;
        Ok(effect.manual_release_handled)
    }

    pub(in crate::app::core) fn observe_completed_connectivity_benchmark_frame(
        &mut self,
        timing: super::super::frame_timing::FrameTimingSnapshot,
    ) -> anyhow::Result<bool> {
        let water = self.water_terrain_status().diagnostics();
        let record = CpuFrameRecord {
            frame: timing.frame,
            total_us: timing.total_ms * 1_000.0,
            gpu_present_us: timing.gpu_present_ms * 1_000.0,
            tracked_us: timing.tracked_cpu_ms * 1_000.0,
            untracked_us: timing.untracked_cpu_ms * 1_000.0,
            terrain_collider_pending: self.terrain_physics.terrain_collider_pending_len(),
            contree_cache_pending: self.contree_builder.cpu_chunk_cache_pending_len(),
            water_source_pending: water.source_pending + water.source_active,
            water_collider_pending: water.collider_pending + water.collider_active,
            water_cache_pending: water.cache_pending + water.cache_active,
            ddgi_ready: self
                .tracer
                .ddgi_ready_for_terrain_revision(self.visible_terrain_revision),
            visible_revision: self.visible_terrain_revision,
        };
        let action = self
            .scenario_owner
            .plan_completed_connectivity_frame(record)?;
        let execution = self.execute_connectivity_action(action);
        let effect = self
            .scenario_owner
            .apply_connectivity_execution(execution)?;
        Ok(effect.observation_complete)
    }

    pub(in crate::app::core) fn advance_connectivity_benchmark(&mut self) -> anyhow::Result<()> {
        let facts = ConnectivityFacts {
            frame: self.time_info.total_frame_count(),
            visible_revision: self.visible_terrain_revision,
            contree_idle: self.contree_builder.cpu_chunk_cache_jobs_idle(),
            terrain_collider_pending: self.terrain_physics.terrain_collider_pending_len(),
            water_ready: self.water_terrain_status().is_ready(),
            ddgi_ready: self
                .tracer
                .ddgi_ready_for_terrain_revision(self.visible_terrain_revision),
            available_particles: self.particle_system.available_capacity(),
        };
        let action = self.scenario_owner.plan_connectivity_action(facts)?;
        let execution = self.execute_connectivity_action(action);
        self.scenario_owner
            .apply_connectivity_execution(execution)?;
        Ok(())
    }

    fn execute_connectivity_action(&mut self, action: ConnectivityAction) -> ConnectivityExecution {
        app_executor::execute(self, action)
    }
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
        }
    }

    fn plan_manual_release(&mut self, facts: ManualReleaseFacts) -> ConnectivityAction {
        if matches!(self.state, BenchState::RetryManualRelease { .. }) {
            let state = std::mem::replace(&mut self.state, BenchState::Complete);
            let BenchState::RetryManualRelease { request, resume } = state else {
                unreachable!("the manual retry phase was checked before it was consumed")
            };
            self.state = *resume;
            return ConnectivityAction::HandleManualRelease(request);
        }
        if matches!(self.state, BenchState::AwaitingManualEdit) {
            ConnectivityAction::HandleManualRelease(ManualReleaseRequest::new(
                ManualReleasePlan::Prepare {
                    frame: facts.frame,
                    revision_before: facts.visible_revision,
                    bound: isolation_bound(),
                    seed: manual_canopy_seed(),
                },
            ))
        } else {
            ConnectivityAction::HandleManualRelease(ManualReleaseRequest::new(
                ManualReleasePlan::Ignore,
            ))
        }
    }

    fn next_action(&mut self, facts: ConnectivityFacts) -> anyhow::Result<ConnectivityAction> {
        let frame = facts.frame;
        if matches!(
            self.state,
            BenchState::RetryManualRelease { .. } | BenchState::RetryCompletedFrame { .. }
        ) {
            return Ok(ConnectivityAction::None);
        }
        if matches!(self.state, BenchState::Commit(_)) {
            let state = std::mem::replace(
                &mut self.state,
                BenchState::AwaitingCommitResult { event_frame: frame },
            );
            let BenchState::Commit(payload) = state else {
                unreachable!("the commit phase was checked before it was consumed")
            };
            return Ok(ConnectivityAction::CommitBounded(payload));
        }

        let mut trace_transition = None;
        let mut atomicity_action = None;
        match &mut self.state {
            BenchState::InstallFixture => {
                return Ok(ConnectivityAction::InstallFixture {
                    manual: self.options.mode == TerrainConnectivityBenchMode::Manual,
                    available_particles: self.options.available_particles,
                });
            }
            BenchState::Warmup { ready_after_frame } => {
                let prior_ready_after_frame = *ready_after_frame;
                let ready = frame >= prior_ready_after_frame
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
                            ready_after_frame: prior_ready_after_frame,
                        };
                        return Ok(ConnectivityAction::ReadBoundedSnapshot {
                            bound,
                            seed: FIXTURE_ORIGIN,
                        });
                    } else {
                        self.state = BenchState::AwaitingReleaseResult {
                            event_frame: frame,
                            ready_after_frame: prior_ready_after_frame,
                        };
                        return Ok(ConnectivityAction::RunReleaseEvent {
                            mode: self.options.mode,
                        });
                    }
                } else if frame.is_multiple_of(120) {
                    log::info!(
                        "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=warmup frame={} ready_after={} contree_idle={} terrain_collider_pending={} water_ready={} ddgi_ready={} revision={}",
                        frame,
                        prior_ready_after_frame,
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
                job,
                release_frame,
                revision_before,
                snapshot_readback_us: _,
                classification_us,
            } => {
                anyhow::ensure!(
                    facts.visible_revision == *revision_before,
                    "bounded topology input revision changed while pending"
                );
                let step_started = Instant::now();
                let step = job.advance(self.options.voxel_budget);
                let step_us = step_started.elapsed().as_secs_f64() * 1_000_000.0;
                *classification_us += step_us;
                log::info!(
                    "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=job_frame mode=bounded release_frame={} frame={} relative={} voxel_budget={} processed={} processed_total={} pending={} step_us={:.0} classification_us={:.0} disposition={} visible_revision={}",
                    *release_frame,
                    frame,
                    frame.saturating_sub(*release_frame),
                    self.options.voxel_budget,
                    step.processed,
                    job.component_len(),
                    job.pending_len(),
                    step_us,
                    *classification_us,
                    step.disposition.label(),
                    facts.visible_revision,
                );
                match step.disposition {
                    BoundedDisposition::Pending => {}
                    BoundedDisposition::Detached => trace_transition = Some(true),
                    BoundedDisposition::Anchored | BoundedDisposition::Deferred => {
                        if self.options.mode == TerrainConnectivityBenchMode::Manual {
                            log::info!(
                                "[TERRAIN_CONNECTIVITY_MANUAL] phase=still_connected disposition={} processed_voxels={} revision={}",
                                step.disposition.label(),
                                job.component_len(),
                                facts.visible_revision,
                            );
                            trace_transition = Some(false);
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
                job,
                release_frame: _,
                revision_before,
                snapshot_readback_us: _,
                classification_us: _,
            } => {
                anyhow::ensure!(facts.visible_revision == *revision_before);
                atomicity_action = Some(ConnectivityAction::ValidateAtomicity {
                    bound: job.bound,
                    component: job.component.clone(),
                    expected_available_particles: self.options.available_particles,
                });
            }
            BenchState::Commit(_) => unreachable!("commit was consumed before matching phases"),
            BenchState::Observing { .. } => {}
            BenchState::AwaitingSnapshotResult { .. }
            | BenchState::AwaitingReleaseResult { .. }
            | BenchState::AwaitingAtomicityResult { .. }
            | BenchState::AwaitingCommitResult { .. } => {
                anyhow::bail!("connectivity bench advanced while awaiting an action result")
            }
            BenchState::RetryManualRelease { .. } | BenchState::RetryCompletedFrame { .. } => {
                unreachable!("retry phases return before the main benchmark FSM advances")
            }
            BenchState::Complete => {}
        }

        if let Some(detached) = trace_transition {
            let state = std::mem::replace(&mut self.state, BenchState::Complete);
            let BenchState::Tracing {
                job,
                release_frame,
                revision_before,
                snapshot_readback_us,
                classification_us,
            } = state
            else {
                unreachable!("only tracing can schedule a topology transition")
            };
            self.state = if detached {
                BenchState::ValidateAtomicity {
                    job,
                    release_frame,
                    revision_before,
                    snapshot_readback_us,
                    classification_us,
                }
            } else {
                BenchState::AwaitingManualEdit
            };
        }

        if let Some(action) = atomicity_action {
            let state = std::mem::replace(&mut self.state, BenchState::Complete);
            let BenchState::ValidateAtomicity {
                job,
                release_frame,
                revision_before,
                snapshot_readback_us,
                classification_us,
            } = state
            else {
                unreachable!("only validated topology can schedule atomicity evidence")
            };
            self.state = BenchState::AwaitingAtomicityResult {
                job,
                release_frame,
                revision_before,
                snapshot_readback_us,
                classification_us,
            };
            return Ok(action);
        }
        Ok(ConnectivityAction::None)
    }

    fn apply_result(&mut self, result: ConnectivityResult) -> anyhow::Result<()> {
        match result {
            ConnectivityResult::None => Ok(()),
            ConnectivityResult::ManualReleaseHandled(outcome) => {
                let prepared = match outcome {
                    Ok(prepared) => prepared,
                    Err(failure) => {
                        anyhow::ensure!(
                            matches!(
                                (failure.request.payload(), &self.state),
                                (
                                    ManualReleasePlan::Prepare { .. },
                                    BenchState::AwaitingManualEdit
                                ) | (ManualReleasePlan::Ignore, _)
                            ),
                            "manual executor returned a request from another owner phase"
                        );
                        let resume = std::mem::replace(&mut self.state, BenchState::Complete);
                        self.state = BenchState::RetryManualRelease {
                            request: failure.request,
                            resume: Box::new(resume),
                        };
                        return Err(failure.error);
                    }
                };
                let Some(prepared) = prepared else {
                    return Ok(());
                };
                anyhow::ensure!(
                    matches!(self.state, BenchState::AwaitingManualEdit),
                    "manual connectivity release completed while state={:?}",
                    self.state,
                );
                self.state = BenchState::Tracing {
                    job: prepared.snapshot.job,
                    release_frame: prepared.frame,
                    revision_before: prepared.revision_before,
                    snapshot_readback_us: prepared.snapshot.snapshot_readback_us,
                    classification_us: 0.0,
                };
                log::info!(
                    "[TERRAIN_CONNECTIVITY_MANUAL] phase=job_start release_frame={} revision={} voxel_budget={}",
                    prepared.frame,
                    prepared.revision_before,
                    self.options.voxel_budget,
                );
                Ok(())
            }
            ConnectivityResult::FixtureInstalled { frame, outcome } => {
                anyhow::ensure!(
                    matches!(self.state, BenchState::InstallFixture),
                    "fixture result does not match bench state {:?}",
                    self.state,
                );
                let installed = match outcome {
                    Ok(installed) => installed,
                    Err(failure) => {
                        anyhow::ensure!(
                            failure.request.manual
                                == (self.options.mode == TerrainConnectivityBenchMode::Manual)
                                && failure.request.available_particles
                                    == self.options.available_particles,
                            "fixture executor returned a request from another action"
                        );
                        return Err(failure.error);
                    }
                };
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
            ConnectivityResult::SnapshotRead(outcome) => {
                let (frame, revision_before, ready_after_frame) = match &self.state {
                    BenchState::AwaitingSnapshotResult {
                        frame,
                        revision_before,
                        ready_after_frame,
                    } => (*frame, *revision_before, *ready_after_frame),
                    state => anyhow::bail!("snapshot result does not match bench state {state:?}"),
                };
                let snapshot = match outcome {
                    Ok(snapshot) => snapshot,
                    Err(failure) => {
                        anyhow::ensure!(
                            failure.request.bound == isolation_bound()
                                && failure.request.seed == FIXTURE_ORIGIN,
                            "snapshot executor returned a request from another action"
                        );
                        self.state = BenchState::Warmup { ready_after_frame };
                        return Err(failure.error);
                    }
                };
                let bound = snapshot.job.bound;
                self.state = BenchState::Tracing {
                    job: snapshot.job,
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
            ConnectivityResult::ReleaseEventRun(outcome) => {
                let (event_frame, ready_after_frame) = match &self.state {
                    BenchState::AwaitingReleaseResult {
                        event_frame,
                        ready_after_frame,
                    } => (*event_frame, *ready_after_frame),
                    state => anyhow::bail!("release result does not match bench state {state:?}"),
                };
                let stages = match outcome {
                    Ok(stages) => stages,
                    Err(failure) => {
                        anyhow::ensure!(
                            failure.request.mode == self.options.mode,
                            "release executor returned a request for another benchmark mode"
                        );
                        self.state = BenchState::Warmup { ready_after_frame };
                        return Err(failure.error);
                    }
                };
                self.stages = Some(stages);
                self.state = BenchState::Observing { event_frame };
                log_event(self.options, event_frame, stages);
                Ok(())
            }
            ConnectivityResult::AtomicityValidated(outcome) => {
                let (release_frame, revision_before, snapshot_readback_us, classification_us) =
                    match &self.state {
                        BenchState::AwaitingAtomicityResult {
                            release_frame,
                            revision_before,
                            snapshot_readback_us,
                            classification_us,
                            ..
                        } => (
                            *release_frame,
                            *revision_before,
                            *snapshot_readback_us,
                            *classification_us,
                        ),
                        state => {
                            anyhow::bail!("atomicity result does not match bench state {state:?}")
                        }
                    };
                let validation = match outcome {
                    Ok(validation) => validation,
                    Err(failure) => {
                        let BenchState::AwaitingAtomicityResult { job, .. } = &self.state else {
                            unreachable!("the atomicity state was checked above")
                        };
                        anyhow::ensure!(
                            failure.request.bound == job.bound
                                && failure.request.component == job.component
                                && failure.request.expected_available_particles
                                    == self.options.available_particles,
                            "atomicity executor returned a request from another topology job"
                        );
                        let state = std::mem::replace(&mut self.state, BenchState::Complete);
                        let BenchState::AwaitingAtomicityResult { job, .. } = state else {
                            unreachable!("the atomicity state was checked above")
                        };
                        self.state = BenchState::ValidateAtomicity {
                            job,
                            release_frame,
                            revision_before,
                            snapshot_readback_us,
                            classification_us,
                        };
                        return Err(failure.error);
                    }
                };
                let BenchState::AwaitingAtomicityResult { job, .. } = &self.state else {
                    unreachable!("the atomicity state was checked above")
                };
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
                let state = std::mem::replace(&mut self.state, BenchState::Complete);
                let BenchState::AwaitingAtomicityResult { mut job, .. } = state else {
                    unreachable!("the atomicity state was checked above")
                };
                let (visual_voxels, sampling_us, staging_clear_us) =
                    prepare_bounded_commit(&mut job, self.options.available_particles);
                self.state = BenchState::Commit(BoundedCommitPayload {
                    job,
                    visual_voxels,
                    release_frame,
                    revision_before,
                    snapshot_readback_us,
                    classification_us,
                    atomic_validation_us: validation.atomic_validation_us,
                    sampling_us,
                    staging_clear_us,
                    manual: self.options.mode == TerrainConnectivityBenchMode::Manual,
                });
                Ok(())
            }
            ConnectivityResult::BoundedCommitted(outcome) => {
                let event_frame = match &self.state {
                    BenchState::AwaitingCommitResult { event_frame } => *event_frame,
                    ref state => {
                        anyhow::bail!("bounded commit result does not match bench state {state:?}")
                    }
                };
                let stages = match outcome {
                    Ok(stages) => stages,
                    Err(failure) => {
                        let BoundedCommitPayload {
                            job,
                            visual_voxels,
                            release_frame,
                            revision_before,
                            snapshot_readback_us,
                            classification_us,
                            atomic_validation_us,
                            sampling_us,
                            staging_clear_us,
                            manual,
                        } = failure.request;
                        anyhow::ensure!(
                            manual == (self.options.mode == TerrainConnectivityBenchMode::Manual),
                            "bounded executor returned a payload for another benchmark mode"
                        );
                        self.state = BenchState::Commit(BoundedCommitPayload {
                            job,
                            visual_voxels,
                            release_frame,
                            revision_before,
                            snapshot_readback_us,
                            classification_us,
                            atomic_validation_us,
                            sampling_us,
                            staging_clear_us,
                            manual,
                        });
                        return Err(failure.error);
                    }
                };
                self.stages = Some(stages);
                self.state = BenchState::Observing { event_frame };
                log_event(self.options, event_frame, stages);
                Ok(())
            }
            ConnectivityResult::CompletedFrameObserved(outcome) => match outcome {
                Ok(observation) => {
                    self.observe_completed_frame_inner(
                        observation.record,
                        observation.fixture_count,
                    )?;
                    Ok(())
                }
                Err(failure) => {
                    let resume = std::mem::replace(&mut self.state, BenchState::Complete);
                    self.state = BenchState::RetryCompletedFrame {
                        request: failure.request,
                        resume: Box::new(resume),
                    };
                    Err(failure.error)
                }
            },
        }
    }

    fn record_gpu_submission(&mut self, frame_slot: usize, frame: u64) {
        if self.gpu_source_frame_by_slot.len() <= frame_slot {
            self.gpu_source_frame_by_slot.resize(frame_slot + 1, None);
        }
        self.gpu_source_frame_by_slot[frame_slot] = Some(frame);
    }

    fn observe_gpu_completion(&mut self, frame_slot: usize, results: &GpuProfilerFrameResults) {
        let source_frame = self
            .gpu_source_frame_by_slot
            .get(frame_slot)
            .copied()
            .flatten();
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
        match &self.state {
            BenchState::InstallFixture
            | BenchState::Warmup { .. }
            | BenchState::AwaitingManualEdit
            | BenchState::Tracing { .. }
            | BenchState::ValidateAtomicity { .. }
            | BenchState::AwaitingSnapshotResult { .. }
            | BenchState::AwaitingReleaseResult { .. }
            | BenchState::AwaitingAtomicityResult { .. }
            | BenchState::Commit(_)
            | BenchState::AwaitingCommitResult { .. } => {
                push_bounded(&mut self.pre_event_gpu, record);
            }
            BenchState::Observing { event_frame } => {
                if source_frame >= *event_frame {
                    log_gpu_frame(*event_frame, record);
                }
            }
            BenchState::RetryManualRelease { .. } => {
                push_bounded(&mut self.pre_event_gpu, record);
            }
            BenchState::RetryCompletedFrame { resume, .. } => match resume.as_ref() {
                BenchState::Observing { event_frame } if source_frame >= *event_frame => {
                    log_gpu_frame(*event_frame, record);
                }
                BenchState::Complete => {}
                _ => push_bounded(&mut self.pre_event_gpu, record),
            },
            BenchState::Complete => {}
        }
    }

    fn plan_completed_frame(&mut self, record: CpuFrameRecord) -> ConnectivityAction {
        if matches!(self.state, BenchState::RetryCompletedFrame { .. }) {
            let state = std::mem::replace(&mut self.state, BenchState::Complete);
            let BenchState::RetryCompletedFrame { request, resume } = state else {
                unreachable!("the completed-frame retry was checked before it was consumed")
            };
            self.state = *resume;
            return ConnectivityAction::ObserveCompletedFrame(request);
        }
        let expected_fixture_solids = match &self.state {
            BenchState::Observing { event_frame }
                if record.frame.saturating_sub(*event_frame)
                    >= u64::from(self.options.observe_frames) =>
            {
                Some(match self.options.mode {
                    TerrainConnectivityBenchMode::Existing => FIXTURE_VOXELS,
                    TerrainConnectivityBenchMode::Correct
                    | TerrainConnectivityBenchMode::Bounded
                    | TerrainConnectivityBenchMode::Manual => 0,
                })
            }
            _ => None,
        };
        ConnectivityAction::ObserveCompletedFrame(CompletedFrameRequest::new(
            record,
            expected_fixture_solids,
        ))
    }

    fn observe_completed_frame_inner(
        &mut self,
        record: CpuFrameRecord,
        fixture_count: Option<FixtureCount>,
    ) -> anyhow::Result<bool> {
        let validation_due = matches!(
            self.state,
            BenchState::Observing { event_frame }
                if record.frame.saturating_sub(event_frame)
                    >= u64::from(self.options.observe_frames)
        );
        let remaining = match (validation_due, fixture_count) {
            (true, Some(count)) => {
                anyhow::ensure!(
                    count.remaining == count.expected,
                    "terrain connectivity benchmark exposed a partial fixture: remaining={} expected={}",
                    count.remaining,
                    count.expected,
                );
                Some(count.remaining)
            }
            (false, None) => None,
            (true, None) => anyhow::bail!("completed connectivity frame lost fixture validation"),
            (false, Some(_)) => {
                anyhow::bail!("connectivity fixture was validated before the observation deadline")
            }
        };
        match &self.state {
            BenchState::InstallFixture
            | BenchState::Warmup { .. }
            | BenchState::AwaitingManualEdit
            | BenchState::Tracing { .. }
            | BenchState::ValidateAtomicity { .. }
            | BenchState::AwaitingSnapshotResult { .. }
            | BenchState::AwaitingReleaseResult { .. }
            | BenchState::AwaitingAtomicityResult { .. }
            | BenchState::Commit(_)
            | BenchState::AwaitingCommitResult { .. } => {
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
                if record.frame == *event_frame {
                    for prior in self.pre_event_cpu.drain(..) {
                        log_cpu_frame(*event_frame, prior);
                    }
                    for prior in self.pre_event_gpu.drain(..) {
                        log_gpu_frame(*event_frame, prior);
                    }
                }
                log_cpu_frame(*event_frame, record);
                if record.frame.saturating_sub(*event_frame)
                    >= u64::from(self.options.observe_frames)
                {
                    let remaining = remaining
                        .context("completed connectivity frame lost validated fixture count")?;
                    let stages = self
                        .stages
                        .context("terrain connectivity benchmark completed without event stages")?;
                    log::info!(
                        "[PERF][TERRAIN_CONNECTIVITY_BENCH] phase=summary mode={} event_frame={} observed_frames={} remaining_fixture_voxels={} disposition={} invalidated_voxels={} spawned_particles={} revision_before={} revision_after={} high_water_terrain_collider={} high_water_contree_cache={} high_water_water_source={} high_water_water_collider={} high_water_water_cache={} ddgi_ready={}",
                        self.options.mode.label(),
                        *event_frame,
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
            BenchState::RetryManualRelease { .. } | BenchState::RetryCompletedFrame { .. } => {
                anyhow::bail!("completed-frame result arrived before its exact retry was replanned")
            }
            BenchState::Complete => Ok(self.options.mode != TerrainConnectivityBenchMode::Manual),
        }
    }
}

impl App {
    fn run_connectivity_release_event(
        &mut self,
        mode: TerrainConnectivityBenchMode,
    ) -> anyhow::Result<EventStages> {
        let app = self;
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
        let visual_voxels = deterministic_visual_sample(
            &component.voxels,
            app.particle_system.available_capacity(),
        );
        let sampling_us = sampling_started.elapsed().as_secs_f64() * 1_000_000.0;
        let sampled_voxels = visual_voxels.len();
        let prepared = PreparedTerrainDetachment::from_cleared_and_visual_voxels(
            &mut app.plain_builder,
            world_dim,
            &component.voxels,
            visual_voxels,
            fixture_bound(),
        )?;
        let committed = prepared.commit(app);

        Ok(EventStages {
            total_us: total_started.elapsed().as_secs_f64() * 1_000_000.0,
            primary_readback_us,
            trace_readback_us,
            classification_us,
            sampling_us,
            invalidation_us: committed.invalidation_us,
            publication_us: committed.publication_us,
            particle_spawn_us: committed.particle_spawn_us,
            classified_voxels: component.voxels.len(),
            trace_readback_tiles,
            invalidated_voxels: committed.detached_voxels,
            sampled_voxels,
            spawned_particles: committed.spawned_particles,
            revision_before,
            revision_after: app.visible_terrain_revision,
            ..EventStages::default()
        })
    }
}

#[derive(Clone, Copy)]
struct BoundedPrepareFacts {
    visible_revision: u32,
    available_particles: usize,
}

struct PreparedBoundedConnectivity {
    detachment: PreparedTerrainDetachment,
    total_started: Instant,
    release_frame: u64,
    revision_before: u32,
    snapshot_readback_us: f64,
    classification_us: f64,
    atomic_validation_us: f64,
    sampling_us: f64,
    staging_clear_us: f64,
    classified_voxels: usize,
    sampled_voxels: usize,
}

impl PreparedBoundedConnectivity {
    fn prepare(
        mut payload: BoundedCommitPayload,
        facts: BoundedPrepareFacts,
    ) -> Result<Self, FailedConnectivityAction<BoundedCommitPayload>> {
        let total_started = Instant::now();
        let validation = (|| {
            anyhow::ensure!(
                facts.visible_revision == payload.revision_before,
                "bounded topology commit revision changed from {} to {}",
                payload.revision_before,
                facts.visible_revision,
            );
            anyhow::ensure!(payload.job.terminal == Some(BoundedDisposition::Detached));
            if !payload.manual {
                anyhow::ensure!(
                    payload.job.component.len() == FIXTURE_VOXELS,
                    "bounded topology commit is not one complete detached fixture"
                );
            }
            anyhow::ensure!(
                facts.available_particles >= payload.visual_voxels.len(),
                "bounded topology visual capacity drifted before commit"
            );
            Ok(())
        })();
        if let Err(error) = validation {
            return Err(FailedConnectivityAction {
                request: payload,
                error,
            });
        }
        let sampled_voxels = payload.visual_voxels.len();
        let atlas_data = std::mem::take(&mut payload.job.snapshot);
        let visual_voxels = std::mem::take(&mut payload.visual_voxels);
        let request = TerrainDetachmentRequest::single_region(
            CHUNK_DIM * VOXEL_DIM_PER_CHUNK,
            payload.job.bound.min(),
            payload.job.bound.dimensions(),
            atlas_data,
            visual_voxels,
            payload.job.component.len(),
            fixture_bound(),
        );
        let detachment = match PreparedTerrainDetachment::prepare(request) {
            Ok(prepared) => prepared,
            Err(rejected) => {
                let (atlas_data, visual_voxels, error) = rejected.into_single_region();
                payload.job.snapshot = atlas_data;
                payload.visual_voxels = visual_voxels;
                return Err(FailedConnectivityAction {
                    request: payload,
                    error,
                });
            }
        };
        let BoundedCommitPayload {
            job,
            visual_voxels,
            release_frame,
            revision_before,
            snapshot_readback_us,
            classification_us,
            atomic_validation_us,
            sampling_us,
            staging_clear_us,
            manual: _,
        } = payload;
        let classified_voxels = job.component.len();
        debug_assert!(visual_voxels.is_empty());
        Ok(Self {
            detachment,
            total_started,
            release_frame,
            revision_before,
            snapshot_readback_us,
            classification_us,
            atomic_validation_us,
            sampling_us,
            staging_clear_us,
            classified_voxels,
            sampled_voxels,
        })
    }

    fn commit(self, app: &mut App) -> EventStages {
        let committed = self.detachment.commit(app);
        EventStages {
            total_us: self.total_started.elapsed().as_secs_f64() * 1_000_000.0,
            primary_readback_us: self.snapshot_readback_us,
            classification_us: self.classification_us,
            sampling_us: self.sampling_us,
            staging_clear_us: self.staging_clear_us,
            invalidation_us: committed.invalidation_us,
            publication_us: committed.publication_us,
            particle_spawn_us: committed.particle_spawn_us,
            classified_voxels: self.classified_voxels,
            invalidated_voxels: committed.detached_voxels,
            sampled_voxels: self.sampled_voxels,
            spawned_particles: committed.spawned_particles,
            revision_before: self.revision_before,
            revision_after: app.visible_terrain_revision,
            release_to_commit_frames: app
                .time_info
                .total_frame_count()
                .saturating_sub(self.release_frame),
            atomic_validation_us: self.atomic_validation_us,
            ..EventStages::default()
        }
    }
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

struct FixtureAtlasWrite {
    origin: UVec3,
    dim: UVec3,
    data: Vec<u8>,
}

impl FixtureAtlasWrite {
    fn new(world_dim: UVec3, origin: UVec3, dim: UVec3, data: Vec<u8>) -> anyhow::Result<Self> {
        anyhow::ensure!(dim.cmpgt(UVec3::ZERO).all());
        anyhow::ensure!(origin.cmple(world_dim).all() && dim.cmple(world_dim - origin).all());
        anyhow::ensure!(
            data.len() == usize::try_from(voxel_count(UAabb3::new(origin, origin + dim)))?
        );
        Ok(Self { origin, dim, data })
    }
}

struct PreparedFixtureInstallation {
    atlas_writes: Vec<FixtureAtlasWrite>,
    publications: Vec<VisibleTerrainPublication>,
    manual_camera: Option<(Vec3, Vec3)>,
    available_particles: usize,
    started: Instant,
}

impl App {
    fn prepare_connectivity_fixture_installation(
        &mut self,
        request: FixtureInstallRequest,
    ) -> Result<PreparedFixtureInstallation, FailedConnectivityAction<FixtureInstallRequest>> {
        let started = Instant::now();
        let prepared = (|| {
            anyhow::ensure!(request.available_particles <= PARTICLE_CAPACITY);
            let world_dim = CHUNK_DIM * VOXEL_DIM_PER_CHUNK;
            let mut atlas_writes = Vec::with_capacity(if request.manual { 3 } else { 2 });
            let mut publications = Vec::with_capacity(if request.manual { 2 } else { 1 });

            let isolation = isolation_bound();
            let isolation_data = vec![0; voxel_count(isolation) as usize];
            atlas_writes.push(FixtureAtlasWrite::new(
                world_dim,
                isolation.min(),
                isolation.dimensions(),
                isolation_data,
            )?);
            let fixture = generate_hollow_canopy();
            atlas_writes.push(FixtureAtlasWrite::new(
                world_dim,
                fixture_bound().min(),
                fixture_bound().dimensions(),
                fixture,
            )?);
            let change =
                VisibleTerrainChange::from_build_edits(vec![BuildEdit::RebuildMeshWithoutFlora(
                    isolation,
                )])?
                .context("fixture installation has no visible terrain chunks")?;
            publications.push(VisibleTerrainPublication::edit(change)?);

            let manual_camera = if request.manual {
                let bound = manual_support_bound();
                atlas_writes.push(FixtureAtlasWrite::new(
                    world_dim,
                    bound.min(),
                    bound.dimensions(),
                    generate_manual_support(),
                )?);
                let change = VisibleTerrainChange::from_build_edits(vec![
                    BuildEdit::RebuildMeshWithoutFlora(bound),
                ])?
                .context("manual support installation has no visible terrain chunks")?;
                publications.push(VisibleTerrainPublication::edit(change)?);
                let center = manual_support_center().as_vec3() / VOXELS_PER_WORLD_UNIT;
                let target = center + Vec3::Y * 0.035;
                Some((target + Vec3::new(-0.24, 0.04, -0.28), target))
            } else {
                None
            };

            Ok(PreparedFixtureInstallation {
                atlas_writes,
                publications,
                manual_camera,
                available_particles: request.available_particles,
                started,
            })
        })();
        prepared.map_err(|error| FailedConnectivityAction { request, error })
    }
}

impl PreparedFixtureInstallation {
    fn commit(self, app: &mut App) -> FixtureInstallResult {
        for write in self.atlas_writes {
            app.plain_builder
                .write_chunk_atlas_region(write.origin, write.dim, &write.data)
                .unwrap_or_else(|error| {
                    panic!("connectivity fixture atlas commit failed after preflight: {error:#}")
                });
        }
        app.plain_builder.mark_all_solid_workgroups_dirty();
        for publication in self.publications {
            app.commit_prepared_visible_terrain(publication);
        }
        if let Some((position, target)) = self.manual_camera {
            app.camera_control.apply_snapshot_mode(true);
            app.camera_control.set_orbit_focus(target);
            assert!(
                app.tracer.set_camera_pose_looking_at(position, target),
                "manual connectivity camera failed after fixture preflight"
            );
        }

        let reserve_started = Instant::now();
        app.particle_system = ParticleSystem::new(PARTICLE_CAPACITY);
        app.particle_snapshots.clear();
        app.terrain_harvest_particle_handles.clear();
        let reserved = PARTICLE_CAPACITY - self.available_particles;
        for index in 0..reserved {
            app.particle_system
                .spawn(ParticleSpawn {
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
                })
                .unwrap_or_else(|| {
                    panic!("reserved particle slot {index} disappeared after capacity preflight")
                });
        }
        assert_eq!(
            app.particle_system.available_capacity(),
            self.available_particles,
            "connectivity fixture particle reservation diverged after preflight"
        );

        FixtureInstallResult {
            setup_us: self.started.elapsed().as_secs_f64() * 1_000_000.0,
            reserve_us: reserve_started.elapsed().as_secs_f64() * 1_000_000.0,
            available_particles: app.particle_system.available_capacity(),
            visible_revision: app.visible_terrain_revision,
        }
    }
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

    fn bounded_commit_payload(revision_before: u32) -> BoundedCommitPayload {
        let mut job = fixture_job();
        job.terminal = Some(BoundedDisposition::Detached);
        job.component = vec![job.index_of(FIXTURE_ORIGIN - job.bound.min())];
        BoundedCommitPayload {
            job,
            visual_voxels: vec![(FIXTURE_ORIGIN, 7)],
            release_frame: 19,
            revision_before,
            snapshot_readback_us: 2.0,
            classification_us: 3.0,
            atomic_validation_us: 4.0,
            sampling_us: 5.0,
            staging_clear_us: 6.0,
            manual: true,
        }
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
    fn production_bounded_prepare_failure_restores_exact_owner_payload() {
        let mut bench = TerrainConnectivityBench::new(TerrainConnectivityBenchOptions {
            mode: TerrainConnectivityBenchMode::Manual,
            available_particles: 8,
            warmup_frames: 1,
            observe_frames: 1,
            voxel_budget: 8,
        });
        bench.state = BenchState::Commit(bounded_commit_payload(7));
        let facts = ConnectivityFacts {
            frame: 23,
            visible_revision: 7,
            contree_idle: true,
            terrain_collider_pending: 0,
            water_ready: true,
            ddgi_ready: true,
            available_particles: 8,
        };
        let payload = match bench.next_action(facts).unwrap() {
            ConnectivityAction::CommitBounded(payload) => payload,
            _ => panic!("commit phase did not move its payload into the action"),
        };
        let snapshot_address = payload.job.snapshot.as_ptr();
        let visual_address = payload.visual_voxels.as_ptr();
        let failure = match PreparedBoundedConnectivity::prepare(
            payload,
            BoundedPrepareFacts {
                visible_revision: 99,
                available_particles: 8,
            },
        ) {
            Ok(_) => panic!("stale revision unexpectedly prepared"),
            Err(failure) => failure,
        };

        let error = bench
            .apply_result(ConnectivityResult::BoundedCommitted(Err(failure)))
            .unwrap_err();
        assert!(error.to_string().contains("revision changed"));
        let retried = match bench.next_action(facts).unwrap() {
            ConnectivityAction::CommitBounded(payload) => payload,
            _ => panic!("owner did not replan the rejected physical payload"),
        };
        assert_eq!(retried.job.snapshot.as_ptr(), snapshot_address);
        assert_eq!(retried.visual_voxels.as_ptr(), visual_address);
    }

    #[test]
    fn production_bounded_atlas_failure_returns_exact_payload_without_owner_progress() {
        let mut bench = TerrainConnectivityBench::new(TerrainConnectivityBenchOptions {
            mode: TerrainConnectivityBenchMode::Manual,
            available_particles: 8,
            warmup_frames: 1,
            observe_frames: 1,
            voxel_budget: 8,
        });
        let mut payload = bounded_commit_payload(7);
        payload.job.snapshot.pop();
        bench.state = BenchState::Commit(payload);
        let facts = ConnectivityFacts {
            frame: 23,
            visible_revision: 7,
            contree_idle: true,
            terrain_collider_pending: 0,
            water_ready: true,
            ddgi_ready: true,
            available_particles: 8,
        };
        let payload = match bench.next_action(facts).unwrap() {
            ConnectivityAction::CommitBounded(payload) => payload,
            _ => panic!("commit phase did not move its payload into the action"),
        };
        let snapshot_address = payload.job.snapshot.as_ptr();
        let visual_address = payload.visual_voxels.as_ptr();
        let failure = match PreparedBoundedConnectivity::prepare(
            payload,
            BoundedPrepareFacts {
                visible_revision: 7,
                available_particles: 8,
            },
        ) {
            Ok(_) => panic!("invalid atlas payload unexpectedly prepared"),
            Err(failure) => failure,
        };

        bench
            .apply_result(ConnectivityResult::BoundedCommitted(Err(failure)))
            .unwrap_err();
        let retried = match bench.next_action(facts).unwrap() {
            ConnectivityAction::CommitBounded(payload) => payload,
            _ => panic!("owner did not replan the rejected atlas payload"),
        };
        assert_eq!(retried.job.snapshot.as_ptr(), snapshot_address);
        assert_eq!(retried.visual_voxels.as_ptr(), visual_address);
    }

    #[test]
    fn failed_physical_result_does_not_commit_connectivity_owner_state() {
        let mut bench = TerrainConnectivityBench::new(TerrainConnectivityBenchOptions {
            mode: TerrainConnectivityBenchMode::Bounded,
            available_particles: 8,
            warmup_frames: 1,
            observe_frames: 1,
            voxel_budget: 8,
        });
        let facts = ConnectivityFacts {
            frame: 23,
            visible_revision: 7,
            contree_idle: true,
            terrain_collider_pending: 0,
            water_ready: true,
            ddgi_ready: true,
            available_particles: 8,
        };
        let request = match bench.next_action(facts).unwrap() {
            ConnectivityAction::InstallFixture {
                manual,
                available_particles,
            } => FixtureInstallRequest {
                manual,
                available_particles,
            },
            _ => panic!("initial connectivity action did not install the fixture"),
        };
        let error = bench
            .apply_result(ConnectivityResult::FixtureInstalled {
                frame: 23,
                outcome: Err(FailedConnectivityAction {
                    request,
                    error: anyhow::anyhow!("injected fixture failure"),
                }),
            })
            .unwrap_err();

        assert!(error.to_string().contains("injected fixture failure"));
        assert!(matches!(bench.state, BenchState::InstallFixture));
        assert!(matches!(
            bench.next_action(facts).unwrap(),
            ConnectivityAction::InstallFixture {
                manual: false,
                available_particles: 8,
            }
        ));
    }

    #[test]
    fn failed_snapshot_read_returns_the_exact_request_for_retry() {
        let mut bench = TerrainConnectivityBench::new(TerrainConnectivityBenchOptions {
            mode: TerrainConnectivityBenchMode::Bounded,
            available_particles: 8,
            warmup_frames: 1,
            observe_frames: 1,
            voxel_budget: 8,
        });
        bench.state = BenchState::Warmup {
            ready_after_frame: 0,
        };
        let facts = ConnectivityFacts {
            frame: 23,
            visible_revision: 7,
            contree_idle: true,
            terrain_collider_pending: 0,
            water_ready: true,
            ddgi_ready: true,
            available_particles: 8,
        };
        let request = match bench.next_action(facts).unwrap() {
            ConnectivityAction::ReadBoundedSnapshot { bound, seed } => {
                SnapshotReadRequest { bound, seed }
            }
            _ => panic!("bounded warmup did not plan a snapshot read"),
        };

        let error = bench
            .apply_result(ConnectivityResult::SnapshotRead(Err(
                FailedConnectivityAction {
                    request,
                    error: anyhow::anyhow!("injected snapshot read failure"),
                },
            )))
            .unwrap_err();

        assert!(error.to_string().contains("injected snapshot read failure"));
        assert!(matches!(
            bench.state,
            BenchState::Warmup {
                ready_after_frame: 0
            }
        ));
        assert!(matches!(
            bench.next_action(facts).unwrap(),
            ConnectivityAction::ReadBoundedSnapshot { bound, seed }
                if bound == isolation_bound() && seed == FIXTURE_ORIGIN
        ));
    }

    #[test]
    fn failed_release_event_restores_the_ready_phase_for_retry() {
        let mut bench = TerrainConnectivityBench::new(TerrainConnectivityBenchOptions {
            mode: TerrainConnectivityBenchMode::Correct,
            available_particles: 8,
            warmup_frames: 1,
            observe_frames: 1,
            voxel_budget: 8,
        });
        bench.state = BenchState::Warmup {
            ready_after_frame: 0,
        };
        let facts = ConnectivityFacts {
            frame: 23,
            visible_revision: 7,
            contree_idle: true,
            terrain_collider_pending: 0,
            water_ready: true,
            ddgi_ready: true,
            available_particles: 8,
        };
        let request = match bench.next_action(facts).unwrap() {
            ConnectivityAction::RunReleaseEvent { mode } => ReleaseEventRequest { mode },
            _ => panic!("ready correct-mode bench did not plan a release event"),
        };

        let error = bench
            .apply_result(ConnectivityResult::ReleaseEventRun(Err(
                FailedConnectivityAction {
                    request,
                    error: anyhow::anyhow!("injected release preflight failure"),
                },
            )))
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("injected release preflight failure"));
        assert!(matches!(
            bench.state,
            BenchState::Warmup {
                ready_after_frame: 0
            }
        ));
        assert!(matches!(
            bench.next_action(facts).unwrap(),
            ConnectivityAction::RunReleaseEvent {
                mode: TerrainConnectivityBenchMode::Correct
            }
        ));
    }

    #[test]
    fn failed_atomicity_validation_replans_the_same_component() {
        let mut bench = TerrainConnectivityBench::new(TerrainConnectivityBenchOptions {
            mode: TerrainConnectivityBenchMode::Bounded,
            available_particles: 8,
            warmup_frames: 1,
            observe_frames: 1,
            voxel_budget: 8,
        });
        let job = fixture_job();
        let expected_component = job.component.clone();
        bench.state = BenchState::ValidateAtomicity {
            job,
            release_frame: 19,
            revision_before: 7,
            snapshot_readback_us: 2.0,
            classification_us: 3.0,
        };
        let facts = ConnectivityFacts {
            frame: 23,
            visible_revision: 7,
            contree_idle: true,
            terrain_collider_pending: 0,
            water_ready: true,
            ddgi_ready: true,
            available_particles: 8,
        };
        let request = match bench.next_action(facts).unwrap() {
            ConnectivityAction::ValidateAtomicity {
                bound,
                component,
                expected_available_particles,
            } => AtomicityValidationRequest {
                bound,
                component,
                expected_available_particles,
            },
            _ => panic!("validated topology did not plan an atomicity check"),
        };

        let error = bench
            .apply_result(ConnectivityResult::AtomicityValidated(Err(
                FailedConnectivityAction {
                    request,
                    error: anyhow::anyhow!("injected atomicity readback failure"),
                },
            )))
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("injected atomicity readback failure"));
        assert!(matches!(
            bench.next_action(facts).unwrap(),
            ConnectivityAction::ValidateAtomicity { component, .. }
                if component == expected_component
        ));
    }

    #[test]
    fn failed_bounded_commit_returns_the_same_large_payload() {
        let mut bench = TerrainConnectivityBench::new(TerrainConnectivityBenchOptions {
            mode: TerrainConnectivityBenchMode::Bounded,
            available_particles: 8,
            warmup_frames: 1,
            observe_frames: 1,
            voxel_budget: 8,
        });
        bench.state = BenchState::Commit(BoundedCommitPayload {
            job: fixture_job(),
            visual_voxels: vec![(UVec3::new(1, 2, 3), 7)],
            release_frame: 19,
            revision_before: 7,
            snapshot_readback_us: 2.0,
            classification_us: 3.0,
            atomic_validation_us: 4.0,
            sampling_us: 5.0,
            staging_clear_us: 6.0,
            manual: false,
        });
        let facts = ConnectivityFacts {
            frame: 23,
            visible_revision: 7,
            contree_idle: true,
            terrain_collider_pending: 0,
            water_ready: true,
            ddgi_ready: true,
            available_particles: 8,
        };
        let payload = match bench.next_action(facts).unwrap() {
            ConnectivityAction::CommitBounded(payload) => payload,
            _ => panic!("commit phase did not move its payload into the action"),
        };
        let snapshot_address = payload.job.snapshot.as_ptr();
        let visual_address = payload.visual_voxels.as_ptr();

        let error = bench
            .apply_result(ConnectivityResult::BoundedCommitted(Err(
                FailedConnectivityAction {
                    request: payload,
                    error: anyhow::anyhow!("injected bounded commit preflight failure"),
                },
            )))
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("injected bounded commit preflight failure"));
        let retried = match bench.next_action(facts).unwrap() {
            ConnectivityAction::CommitBounded(payload) => payload,
            _ => panic!("failed bounded commit was not replanned"),
        };
        assert_eq!(retried.job.snapshot.as_ptr(), snapshot_address);
        assert_eq!(retried.visual_voxels.as_ptr(), visual_address);
    }

    #[test]
    fn manual_release_is_planned_as_an_owned_app_execution() {
        let mut owner = ScenarioOwner::Connectivity(TerrainConnectivityBench::new(
            TerrainConnectivityBenchOptions {
                mode: TerrainConnectivityBenchMode::Manual,
                available_particles: 8,
                warmup_frames: 1,
                observe_frames: 1,
                voxel_budget: 8,
            },
        ));
        let ScenarioOwner::Connectivity(bench) = &mut owner else {
            panic!("test constructed the wrong scenario owner");
        };
        bench.state = BenchState::AwaitingManualEdit;

        let action = owner
            .plan_manual_connectivity_release(ManualReleaseFacts {
                frame: 41,
                visible_revision: 12,
            })
            .unwrap();

        let request = match action {
            ConnectivityAction::HandleManualRelease(request)
                if matches!(
                    request.payload(),
                    ManualReleasePlan::Prepare {
                        frame: 41,
                        revision_before: 12,
                        ..
                    }
                ) =>
            {
                request
            }
            _ => panic!("manual release did not plan an owned preparation"),
        };
        let request_address = request.payload() as *const ManualReleasePlan;
        let ScenarioOwner::Connectivity(bench) = &mut owner else {
            panic!("test constructed the wrong scenario owner");
        };
        let error = bench
            .apply_result(ConnectivityResult::ManualReleaseHandled(Err(
                FailedConnectivityAction {
                    request,
                    error: anyhow::anyhow!("injected manual snapshot failure"),
                },
            )))
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("injected manual snapshot failure"));
        assert!(matches!(
            &owner,
            ScenarioOwner::Connectivity(TerrainConnectivityBench {
                state: BenchState::RetryManualRelease { resume, .. },
                ..
            }) if matches!(resume.as_ref(), BenchState::AwaitingManualEdit)
        ));
        let retried = owner
            .plan_manual_connectivity_release(ManualReleaseFacts {
                frame: 99,
                visible_revision: 77,
            })
            .unwrap();
        let ConnectivityAction::HandleManualRelease(retried) = retried else {
            panic!("failed manual release was not replanned")
        };
        assert_eq!(
            retried.payload() as *const ManualReleasePlan,
            request_address
        );
        assert!(matches!(
            retried.payload(),
            ManualReleasePlan::Prepare {
                frame: 41,
                revision_before: 12,
                ..
            }
        ));
    }

    #[test]
    fn completed_frame_validation_is_an_owned_app_execution() {
        let mut owner = ScenarioOwner::Connectivity(TerrainConnectivityBench::new(
            TerrainConnectivityBenchOptions {
                mode: TerrainConnectivityBenchMode::Bounded,
                available_particles: 8,
                warmup_frames: 1,
                observe_frames: 1,
                voxel_budget: 8,
            },
        ));
        let ScenarioOwner::Connectivity(bench) = &mut owner else {
            panic!("test constructed the wrong scenario owner");
        };
        bench.state = BenchState::Observing { event_frame: 40 };
        bench.stages = Some(EventStages::default());
        let record = CpuFrameRecord {
            frame: 41,
            total_us: 1.0,
            gpu_present_us: 2.0,
            tracked_us: 3.0,
            untracked_us: 4.0,
            terrain_collider_pending: 5,
            contree_cache_pending: 6,
            water_source_pending: 7,
            water_collider_pending: 8,
            water_cache_pending: 9,
            ddgi_ready: true,
            visible_revision: 10,
        };

        let request = match owner.plan_completed_connectivity_frame(record).unwrap() {
            ConnectivityAction::ObserveCompletedFrame(request)
                if request.payload().expected_fixture_solids == Some(0) =>
            {
                request
            }
            _ => panic!("completed frame did not plan fixture validation"),
        };
        let request_address = request.payload() as *const CompletedFramePayload;
        let ScenarioOwner::Connectivity(bench) = &mut owner else {
            panic!("test constructed the wrong scenario owner");
        };
        let error = bench
            .apply_result(ConnectivityResult::CompletedFrameObserved(Err(
                FailedConnectivityAction {
                    request,
                    error: anyhow::anyhow!("injected fixture validation failure"),
                },
            )))
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("injected fixture validation failure"));
        let ScenarioOwner::Connectivity(mut bench) = owner else {
            panic!("test constructed the wrong scenario owner");
        };
        assert!(matches!(
            &bench.state,
            BenchState::RetryCompletedFrame {
                resume,
                ..
            } if matches!(resume.as_ref(), BenchState::Observing { event_frame: 40 })
        ));
        assert_eq!(bench.high_water.terrain_collider, 0);
        let mut changed_record = record;
        changed_record.frame = 99;
        changed_record.visible_revision = 77;
        let ConnectivityAction::ObserveCompletedFrame(retried) =
            bench.plan_completed_frame(changed_record)
        else {
            panic!("failed completed-frame observation was not replanned")
        };
        assert_eq!(
            retried.payload() as *const CompletedFramePayload,
            request_address
        );
        assert_eq!(retried.payload().record.frame, 41);
        assert_eq!(retried.payload().record.visible_revision, 10);
    }

    #[test]
    fn gpu_completion_observes_the_source_frame_from_the_exact_submission_slot() {
        let mut owner = ScenarioOwner::Connectivity(TerrainConnectivityBench::new(
            TerrainConnectivityBenchOptions {
                mode: TerrainConnectivityBenchMode::Bounded,
                available_particles: 8,
                warmup_frames: 1,
                observe_frames: 1,
                voxel_budget: 8,
            },
        ));
        let results = GpuProfilerFrameResults {
            scopes: Vec::new(),
            dropped_scope_count: 0,
            timestamp_period_ns: 1.0,
        };
        owner.record_connectivity_gpu_submission(0, 31);
        owner.record_connectivity_gpu_submission(3, 47);

        owner.observe_connectivity_gpu_completion(3, &results);

        let ScenarioOwner::Connectivity(bench) = owner else {
            panic!("test constructed the wrong scenario owner");
        };
        assert_eq!(
            bench
                .pre_event_gpu
                .back()
                .expect("the exact submitted slot must produce one observation")
                .source_frame,
            47
        );
    }

    #[test]
    fn only_an_active_connectivity_diagnostic_reserves_ambient_particle_capacity() {
        let mut connectivity = ScenarioOwner::Connectivity(TerrainConnectivityBench::new(
            TerrainConnectivityBenchOptions {
                mode: TerrainConnectivityBenchMode::Bounded,
                available_particles: 8,
                warmup_frames: 1,
                observe_frames: 1,
                voxel_budget: 8,
            },
        ));
        let mut garden = ScenarioOwner::Standard(
            super::super::super::launch_owners::StandardScenarioOwner::World(
                super::super::super::launch_owners::WorldScenarioOwner::Garden,
            ),
        );

        assert!(!connectivity.allows_ambient_particle_emitters());
        assert!(garden.allows_ambient_particle_emitters());

        let ScenarioOwner::Connectivity(bench) = &mut connectivity else {
            panic!("test constructed the wrong scenario owner");
        };
        bench.state = BenchState::Complete;
        assert!(connectivity.allows_ambient_particle_emitters());
    }

    #[test]
    fn connectivity_protocol_is_exposed_only_by_the_dedicated_owner_variant() {
        let connectivity = ScenarioOwner::Connectivity(TerrainConnectivityBench::new(
            TerrainConnectivityBenchOptions {
                mode: TerrainConnectivityBenchMode::Bounded,
                available_particles: 8,
                warmup_frames: 1,
                observe_frames: 1,
                voxel_budget: 8,
            },
        ));
        let garden = ScenarioOwner::Standard(
            super::super::super::launch_owners::StandardScenarioOwner::World(
                super::super::super::launch_owners::WorldScenarioOwner::Garden,
            ),
        );

        assert!(matches!(connectivity, ScenarioOwner::Connectivity(_)));
        assert!(matches!(garden, ScenarioOwner::Standard(_)));
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
