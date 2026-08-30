use super::settings::{apply_water_gui_adjustables_to_config, WaterRuntimeOverrides};
use crate::app::GuiAdjustables;
use glam::{IVec3, Vec3};
use re_flora_water::{
    DebugWaterSpawnResult, DebugWaterSpawnSkipReason, PondWaterConfig, PondWaterSim,
    WaterTerrainCacheBuildRequest, WaterTerrainCachePatch, WaterTerrainColliderChunk,
    WaterTerrainColliderSet,
};
use std::{
    sync::{mpsc, Arc, Mutex, TryLockError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(super) const WATER_SIM_THREAD_DEFAULT_MAX_SUBSTEPS: usize = 4;
const WATER_SIM_COMMAND_CHANNEL_CAPACITY: usize = 256;
const WATER_SIM_THREAD_MAX_COMMANDS_PER_TICK: usize = 1024;
const WATER_SIM_THREAD_SNAPSHOT_INTERVAL: Duration = Duration::from_millis(16);
const WATER_SIM_THREAD_IDLE_SLEEP: Duration = Duration::from_millis(16);
// One complete frame every four former bucket intervals keeps the average particle-copy
// rate approximately unchanged without exposing mixed-time particle sets.
const WATER_SIM_COHERENT_FRAME_INTERVAL_MULTIPLIER: u32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WaterSimRuntimeOptions {
    enabled: bool,
    perf_logging: bool,
    max_substeps_per_tick: usize,
    snapshot_interval: Duration,
}

impl Default for WaterSimRuntimeOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            perf_logging: false,
            max_substeps_per_tick: WATER_SIM_THREAD_DEFAULT_MAX_SUBSTEPS,
            snapshot_interval: WATER_SIM_THREAD_SNAPSHOT_INTERVAL,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct WaterParticleFrameSchedule {
    enabled_opportunities_since_publish: u32,
}

impl WaterParticleFrameSchedule {
    fn should_publish(&mut self, runtime_options: WaterSimRuntimeOptions) -> bool {
        if !runtime_options.enabled {
            self.enabled_opportunities_since_publish = 0;
            return true;
        }

        self.enabled_opportunities_since_publish += 1;
        if self.enabled_opportunities_since_publish
            < water_particle_frame_interval_multiplier(runtime_options)
        {
            return false;
        }

        self.enabled_opportunities_since_publish = 0;
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct WaterFramePublishReport {
    frame_revision: u64,
    particle_count: usize,
    published_particles: usize,
    total_ms: f32,
    lock_ms: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct WaterThreadPerfStats {
    report_seconds: f32,
    ticks: u64,
    active_ticks: u64,
    idle_ticks: u64,
    commands: u64,
    max_commands_per_tick: usize,
    maxed_command_ticks: u64,
    command_drain_ms: f64,
    publish_count: u64,
    publish_particles: u64,
    publish_ms: f64,
    publish_lock_ms: f64,
    latest_frame_revision: Option<u64>,
}

impl WaterThreadPerfStats {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn record_tick(&mut self, dt: f32, enabled: bool) {
        self.report_seconds += dt.max(0.0);
        self.ticks += 1;
        if enabled {
            self.active_ticks += 1;
        } else {
            self.idle_ticks += 1;
        }
    }

    fn record_command_drain(&mut self, commands_this_tick: usize, elapsed: Duration) {
        self.commands += commands_this_tick as u64;
        self.max_commands_per_tick = self.max_commands_per_tick.max(commands_this_tick);
        if commands_this_tick >= WATER_SIM_THREAD_MAX_COMMANDS_PER_TICK {
            self.maxed_command_ticks += 1;
        }
        self.command_drain_ms += elapsed.as_secs_f64() * 1000.0;
    }

    fn record_publish(&mut self, report: WaterFramePublishReport) {
        self.publish_count += 1;
        self.publish_particles += report.published_particles as u64;
        self.publish_ms += report.total_ms as f64;
        self.publish_lock_ms += report.lock_ms as f64;
        self.latest_frame_revision = Some(report.frame_revision);
    }

    fn log_and_reset_if_due(
        &mut self,
        sim: &PondWaterSim,
        runtime_options: WaterSimRuntimeOptions,
    ) {
        if self.report_seconds < 1.0 {
            return;
        }

        let ticks = self.ticks.max(1) as f64;
        let publishes = self.publish_count.max(1) as f64;
        log::info!(
            "[PERF][WATER_THREAD] seconds={:.3} enabled={} particles={} ticks={} active_ticks={} idle_ticks={} commands={} commands_per_tick={:.2} max_commands_per_tick={} maxed_command_ticks={} command_drain={:.3}ms publish_count={} publishes_per_second={:.2} publish_particles={} publish_particles_per_publish={:.1} publish_particles_per_second={:.1} publish={:.3}ms publish_lock={:.3}ms coherent_frame_interval_multiplier={} latest_frame_revision={:?}",
            self.report_seconds,
            runtime_options.enabled,
            sim.particles.len(),
            self.ticks,
            self.active_ticks,
            self.idle_ticks,
            self.commands,
            self.commands as f64 / ticks,
            self.max_commands_per_tick,
            self.maxed_command_ticks,
            self.command_drain_ms,
            self.publish_count,
            self.publish_count as f64 / self.report_seconds.max(f32::EPSILON) as f64,
            self.publish_particles,
            self.publish_particles as f64 / publishes,
            self.publish_particles as f64 / self.report_seconds.max(f32::EPSILON) as f64,
            self.publish_ms,
            self.publish_lock_ms,
            water_particle_frame_interval_multiplier(runtime_options),
            self.latest_frame_revision,
        );
        self.reset();
    }
}

enum WaterSimCommand {
    UpdateConfig(PondWaterConfig),
    SetRuntimeOptions(WaterSimRuntimeOptions),
    PauseAndAcknowledge(mpsc::SyncSender<()>),
    UpsertTerrainColliderChunkDeferred(WaterTerrainColliderChunk),
    RemoveTerrainColliderChunkDeferred(IVec3),
    FinishTerrainColliderChunkBatch {
        stabilize_particles: bool,
    },
    InvalidateTerrainGridCacheForChunk(IVec3),
    ApplyTerrainGridCachePatch(WaterTerrainCachePatch),
    StabilizeAfterTerrainChunkChange(IVec3),
    #[allow(dead_code)]
    SpawnDebugParticlesAtSurface {
        surface_point_ws: Vec3,
        count: usize,
        radius_ws: f32,
    },
    Shutdown,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::app::core) struct WaterSimParticleSnapshot {
    pub(in crate::app::core) position_ws: Vec3,
    pub(in crate::app::core) velocity: Vec3,
}

#[derive(Clone, Debug)]
pub(in crate::app::core) struct WaterParticleFrame {
    revision: u64,
    particles: Vec<WaterSimParticleSnapshot>,
    sim_time_seconds: f32,
    worker_update_ms: f32,
    worker_substeps: u32,
}

impl WaterParticleFrame {
    fn can_replace(&self, current: Option<&Self>) -> bool {
        if !self.sim_time_seconds.is_finite() {
            return false;
        }
        current.is_none_or(|current| {
            self.revision > current.revision && self.sim_time_seconds >= current.sim_time_seconds
        })
    }

    pub(in crate::app::core) fn revision(&self) -> u64 {
        self.revision
    }

    pub(in crate::app::core) fn sim_time_seconds(&self) -> f32 {
        self.sim_time_seconds
    }

    pub(in crate::app::core) fn particles(&self) -> &[WaterSimParticleSnapshot] {
        &self.particles
    }
}

#[derive(Default)]
struct WaterSimThreadShared {
    latest_frame: Option<WaterParticleFrame>,
}

pub(super) struct AsyncWaterSim {
    pub(super) config: PondWaterConfig,
    pub(super) dx: f32,
    terrain: Option<WaterTerrainColliderSet>,
    command_tx: mpsc::SyncSender<WaterSimCommand>,
    shared: Arc<Mutex<WaterSimThreadShared>>,
    worker: Option<JoinHandle<()>>,
    latest_frame: Option<WaterParticleFrame>,
    snapshot_poll_accumulator: f32,
    last_sent_config: PondWaterConfig,
    runtime_options: WaterSimRuntimeOptions,
}

impl AsyncWaterSim {
    pub(super) fn new(config: PondWaterConfig) -> Self {
        let dx = water_config_dx(&config);
        let (command_tx, command_rx) = mpsc::sync_channel(WATER_SIM_COMMAND_CHANNEL_CAPACITY);
        let shared = Arc::new(Mutex::new(WaterSimThreadShared::default()));
        let worker_shared = Arc::clone(&shared);
        let worker_config = config.clone();
        let worker = thread::Builder::new()
            .name("water-sim".to_owned())
            .spawn(move || run_water_sim_thread(worker_config, command_rx, worker_shared))
            .expect("failed to spawn water simulation thread");
        Self {
            config: config.clone(),
            dx,
            terrain: None,
            command_tx,
            shared,
            worker: Some(worker),
            latest_frame: None,
            snapshot_poll_accumulator: 0.0,
            last_sent_config: config,
            runtime_options: WaterSimRuntimeOptions::default(),
        }
    }

    pub(super) fn apply_gui_adjustables(
        &mut self,
        gui_adjustables: &GuiAdjustables,
        runtime_overrides: &WaterRuntimeOverrides,
    ) {
        apply_water_gui_adjustables_to_config(&mut self.config, gui_adjustables);
        runtime_overrides.apply(&mut self.config);
        self.dx = water_config_dx(&self.config);
        if self.config != self.last_sent_config {
            let config = self.config.clone();
            if self.try_send_coalescable_command(WaterSimCommand::UpdateConfig(config.clone())) {
                self.last_sent_config = config;
            }
        }
    }

    pub(super) fn set_runtime_options(
        &mut self,
        enabled: bool,
        perf_logging: bool,
        max_substeps_per_tick: usize,
        snapshot_interval: Duration,
    ) {
        let options = WaterSimRuntimeOptions {
            enabled,
            perf_logging,
            max_substeps_per_tick: max_substeps_per_tick.max(1),
            snapshot_interval: snapshot_interval.max(Duration::from_secs_f32(1.0 / 240.0)),
        };
        if options == self.runtime_options {
            return;
        }
        if self.try_send_coalescable_command(WaterSimCommand::SetRuntimeOptions(options)) {
            self.runtime_options = options;
        }
    }

    /// Establishes a worker-observed quiescence boundary before terrain replacement.
    pub(super) fn pause_and_wait(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.worker.is_some(),
            "water simulation worker is not available"
        );
        let (ack_tx, ack_rx) = mpsc::sync_channel(0);
        self.command_tx
            .send(WaterSimCommand::PauseAndAcknowledge(ack_tx))
            .map_err(|_| anyhow::anyhow!("water simulation command channel disconnected"))?;
        ack_rx
            .recv()
            .map_err(|_| anyhow::anyhow!("water simulation worker stopped before pausing"))?;
        self.runtime_options.enabled = false;
        Ok(())
    }

    pub(super) fn poll_latest_particle_frame_after_frame(
        &mut self,
        frame_delta_time: f32,
        water_tick_seconds: f32,
    ) {
        self.snapshot_poll_accumulator += frame_delta_time.max(0.0);
        let water_tick_elapsed = self.snapshot_poll_accumulator >= water_tick_seconds;
        if water_tick_elapsed {
            self.snapshot_poll_accumulator %= water_tick_seconds;
        }
        if water_tick_elapsed || self.latest_frame.is_none() {
            self.poll_latest_particle_frame();
        }
    }

    fn poll_latest_particle_frame(&mut self) {
        let latest = match self.shared.try_lock() {
            Ok(mut guard) => guard.latest_frame.take(),
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner().latest_frame.take(),
            Err(TryLockError::WouldBlock) => None,
        };
        if let Some(frame) = latest {
            if frame.can_replace(self.latest_frame.as_ref()) {
                self.latest_frame = Some(frame);
            }
        }
    }

    pub(super) fn latest_particle_frame(&self) -> Option<&WaterParticleFrame> {
        self.latest_frame.as_ref()
    }

    pub(super) fn status_text(&self, handoff_main_thread_ms: Option<f32>) -> String {
        let Some(frame) = self.latest_frame.as_ref() else {
            return "Water sim thread: --".to_owned();
        };
        match handoff_main_thread_ms {
            Some(handoff_ms) => format!(
                "Water sim thread: handoff {:.3} ms, worker {:.3} ms, substeps {}, frame {}, particles {}, sim {:.2}s",
                handoff_ms,
                frame.worker_update_ms,
                frame.worker_substeps,
                frame.revision(),
                frame.particles().len(),
                frame.sim_time_seconds(),
            ),
            None => format!(
                "Water sim thread: worker {:.3} ms, substeps {}, frame {}, particles {}, sim {:.2}s",
                frame.worker_update_ms,
                frame.worker_substeps,
                frame.revision(),
                frame.particles().len(),
                frame.sim_time_seconds(),
            ),
        }
    }

    pub(super) fn terrain_collider_set(&self) -> Option<&WaterTerrainColliderSet> {
        self.terrain.as_ref()
    }

    pub(super) fn terrain_grid_cache_build_request_for_chunk(
        &self,
        chunk_id: IVec3,
    ) -> Option<WaterTerrainCacheBuildRequest> {
        WaterTerrainCacheBuildRequest::for_config_and_terrain(
            &self.config,
            self.terrain.clone(),
            chunk_id,
        )
    }

    pub(super) fn upsert_terrain_collider_chunk_deferred(
        &mut self,
        chunk: WaterTerrainColliderChunk,
    ) {
        self.terrain
            .get_or_insert_with(WaterTerrainColliderSet::new)
            .insert_chunk(Arc::new(chunk.clone()));
        self.send_critical_command(WaterSimCommand::UpsertTerrainColliderChunkDeferred(chunk));
    }

    pub(super) fn remove_terrain_collider_chunk_deferred(&mut self, chunk_id: IVec3) -> bool {
        let Some(terrain) = self.terrain.as_mut() else {
            return false;
        };
        if terrain.remove_chunk(chunk_id).is_none() {
            return false;
        }
        if terrain.is_empty() {
            self.terrain = None;
        }
        self.send_critical_command(WaterSimCommand::RemoveTerrainColliderChunkDeferred(
            chunk_id,
        ));
        true
    }

    pub(super) fn finish_terrain_collider_chunk_batch(&mut self, stabilize_particles: bool) {
        self.send_critical_command(WaterSimCommand::FinishTerrainColliderChunkBatch {
            stabilize_particles,
        });
    }

    pub(super) fn invalidate_terrain_grid_cache_for_chunk(&mut self, chunk_id: IVec3) {
        self.send_critical_command(WaterSimCommand::InvalidateTerrainGridCacheForChunk(
            chunk_id,
        ));
    }

    pub(super) fn submit_terrain_grid_cache_patch(&mut self, patch: WaterTerrainCachePatch) {
        self.send_critical_command(WaterSimCommand::ApplyTerrainGridCachePatch(patch));
    }

    pub(super) fn stabilize_after_terrain_chunk_change(&mut self, chunk_id: IVec3) {
        self.send_critical_command(WaterSimCommand::StabilizeAfterTerrainChunkChange(chunk_id));
    }

    /// Stops the simulation worker at the explicit application-shutdown
    /// boundary.  Calling this more than once is harmless; `Drop` uses the
    /// same path as a fallback for construction sites that do not run the
    /// normal event-loop termination handler.
    pub(super) fn shutdown(&mut self) {
        if self.worker.is_none() {
            return;
        }

        match self.command_tx.try_send(WaterSimCommand::Shutdown) {
            Ok(()) | Err(mpsc::TrySendError::Disconnected(_)) => {}
            Err(mpsc::TrySendError::Full(command)) => {
                let _ = self.command_tx.send(command);
            }
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                log::warn!("[WATER][THREAD] water simulation thread panicked during shutdown");
            }
        }
    }

    #[allow(dead_code)]
    pub(super) fn request_debug_particle_spawn(
        &mut self,
        surface_point_ws: Vec3,
        count: usize,
        radius_ws: f32,
    ) {
        self.send_critical_command(WaterSimCommand::SpawnDebugParticlesAtSurface {
            surface_point_ws,
            count,
            radius_ws,
        });
    }

    fn try_send_coalescable_command(&self, command: WaterSimCommand) -> bool {
        match self.command_tx.try_send(command) {
            Ok(()) => true,
            Err(mpsc::TrySendError::Full(_)) => {
                log::debug!("[WATER][THREAD] dropped coalescable command: command queue full");
                false
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                log::warn!("[WATER][THREAD] command queue disconnected");
                false
            }
        }
    }

    fn send_critical_command(&self, command: WaterSimCommand) {
        match self.command_tx.try_send(command) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(command)) => {
                log::warn!("[WATER][THREAD] command queue full; blocking for critical command");
                if let Err(err) = self.command_tx.send(command) {
                    log::warn!("[WATER][THREAD] failed to send critical command: {}", err);
                }
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                log::warn!("[WATER][THREAD] command queue disconnected");
            }
        }
    }

    #[cfg(test)]
    pub(super) fn worker_stopped_for_test(&self) -> bool {
        self.worker.is_none()
    }
}

impl Drop for AsyncWaterSim {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn water_config_dx(config: &PondWaterConfig) -> f32 {
    let extent_ws = config.collider.extent();
    (extent_ws.x / config.grid_dim.x as f32)
        .max(extent_ws.y / config.grid_dim.y as f32)
        .max(extent_ws.z / config.grid_dim.z as f32)
}

fn run_water_sim_thread(
    config: PondWaterConfig,
    command_rx: mpsc::Receiver<WaterSimCommand>,
    shared: Arc<Mutex<WaterSimThreadShared>>,
) {
    let mut sim = PondWaterSim::new(config);
    let mut runtime_options = WaterSimRuntimeOptions::default();
    let mut last_tick = Instant::now();
    let mut last_publish = Instant::now();
    let mut next_frame_revision = 0u64;
    let mut frame_schedule = WaterParticleFrameSchedule::default();
    let mut thread_perf_stats = WaterThreadPerfStats::default();
    let _ = publish_water_particle_frame(&sim, &shared, next_frame_revision, 0.0, 0, false);
    next_frame_revision = next_frame_revision
        .checked_add(1)
        .expect("water particle frame revision overflowed");

    loop {
        let tick_start = Instant::now();
        let command_drain_start = runtime_options.perf_logging.then(Instant::now);
        let mut commands_this_tick = 0usize;
        for _ in 0..WATER_SIM_THREAD_MAX_COMMANDS_PER_TICK {
            match command_rx.try_recv() {
                Ok(command) => {
                    commands_this_tick += 1;
                    if !handle_water_sim_command(&mut sim, command, &mut runtime_options) {
                        log::info!("[WATER][THREAD] simulation thread stopped");
                        return;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    log::info!("[WATER][THREAD] simulation command channel disconnected");
                    return;
                }
            }
        }
        if let Some(command_drain_start) = command_drain_start {
            thread_perf_stats
                .record_command_drain(commands_this_tick, command_drain_start.elapsed());
        }

        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;
        if runtime_options.perf_logging {
            thread_perf_stats.record_tick(dt, runtime_options.enabled);
        } else {
            thread_perf_stats.reset();
        }
        let sim_update_start = Instant::now();
        let sim_time_before = sim.sim_time_seconds;
        if runtime_options.enabled {
            sim.update_with_max_substeps(
                dt,
                runtime_options.perf_logging,
                runtime_options.max_substeps_per_tick,
            );
        }
        let worker_update_ms = sim_update_start.elapsed().as_secs_f32() * 1000.0;
        let substep_dt = sim.config.substep_dt.max(1.0e-6);
        let worker_substeps =
            ((sim.sim_time_seconds - sim_time_before).max(0.0) / substep_dt).round() as u32;

        if last_publish.elapsed() >= runtime_options.snapshot_interval {
            if frame_schedule.should_publish(runtime_options) {
                let publish_report = publish_water_particle_frame(
                    &sim,
                    &shared,
                    next_frame_revision,
                    worker_update_ms,
                    worker_substeps,
                    runtime_options.perf_logging,
                );
                if runtime_options.perf_logging {
                    thread_perf_stats.record_publish(publish_report);
                }
                next_frame_revision = next_frame_revision
                    .checked_add(1)
                    .expect("water particle frame revision overflowed");
            }
            last_publish = Instant::now();
        }

        if runtime_options.perf_logging {
            thread_perf_stats.log_and_reset_if_due(&sim, runtime_options);
        }

        let sleep_for =
            water_sim_thread_sleep_duration(&sim, runtime_options, tick_start.elapsed());
        match command_rx.recv_timeout(sleep_for) {
            Ok(command) => {
                if runtime_options.perf_logging {
                    thread_perf_stats.record_command_drain(1, Duration::ZERO);
                }
                if !handle_water_sim_command(&mut sim, command, &mut runtime_options) {
                    log::info!("[WATER][THREAD] simulation thread stopped");
                    return;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                log::info!("[WATER][THREAD] simulation command channel disconnected");
                return;
            }
        }
    }
}

fn handle_water_sim_command(
    sim: &mut PondWaterSim,
    command: WaterSimCommand,
    runtime_options: &mut WaterSimRuntimeOptions,
) -> bool {
    match command {
        WaterSimCommand::UpdateConfig(config) => {
            sim.config.substep_dt = config.substep_dt;
            sim.config.particle_mass = config.particle_mass;
            sim.config.particle_volume = config.particle_volume;
            sim.config.terrain_collision_margin_cells = config.terrain_collision_margin_cells;
            sim.config.terrain_density_min_fluid_fraction =
                config.terrain_density_min_fluid_fraction;
            sim.config.terrain_density_max_correction_factor =
                config.terrain_density_max_correction_factor;
            sim.config.terrain_density_occupancy_transition_cells =
                config.terrain_density_occupancy_transition_cells;
            sim.config.quiet_settling_velocity_damping_per_sec =
                config.quiet_settling_velocity_damping_per_sec;
            sim.config.quiet_settling_affine_damping_per_sec =
                config.quiet_settling_affine_damping_per_sec;
            sim.config.terrain_tangent_damping_per_sec = config.terrain_tangent_damping_per_sec;
            sim.config.linear_damping_per_sec = config.linear_damping_per_sec;
            sim.config.debug_spawn_height_offset = config.debug_spawn_height_offset;
            sim.config.gravity = config.gravity;
            sim.config.stiffness = config.stiffness;
            sim.config.gamma = config.gamma;
            sim.config.j_min = config.j_min;
            sim.config.dynamic_viscosity = config.dynamic_viscosity;
            sim.config.pressure_floor = config.pressure_floor;
            sim.config.wall_damping = config.wall_damping;
        }
        WaterSimCommand::SetRuntimeOptions(options) => {
            *runtime_options = options;
        }
        WaterSimCommand::PauseAndAcknowledge(ack_tx) => {
            runtime_options.enabled = false;
            let _ = ack_tx.send(());
        }
        WaterSimCommand::UpsertTerrainColliderChunkDeferred(chunk) => {
            sim.upsert_terrain_collider_chunk_deferred(chunk);
        }
        WaterSimCommand::RemoveTerrainColliderChunkDeferred(chunk_id) => {
            sim.remove_terrain_collider_chunk_deferred(chunk_id);
        }
        WaterSimCommand::FinishTerrainColliderChunkBatch {
            stabilize_particles,
        } => {
            sim.finish_terrain_collider_chunk_batch(stabilize_particles);
        }
        WaterSimCommand::InvalidateTerrainGridCacheForChunk(chunk_id) => {
            sim.invalidate_terrain_grid_cache_for_chunk(chunk_id);
        }
        WaterSimCommand::ApplyTerrainGridCachePatch(patch) => {
            if let Some(report) = sim.apply_terrain_grid_cache_patch(patch) {
                log::info!(
                    "[WATER][TERRAIN_CACHE] applied worker grid cache region chunk={:?} chunks={} grid {:?} range {:?}..{:?} nodes={} has_sdf={} near_surface={} normals={} band={:.5} dx={:.5} worker_ms={:.2} apply_ms={:.3}",
                    report.chunk_id,
                    report.terrain_chunk_count,
                    report.grid_dim,
                    report.min_node,
                    report.max_node_exclusive,
                    report.node_count,
                    report.has_sdf_count,
                    report.near_surface_count,
                    report.normal_count,
                    report.near_surface_band,
                    report.dx,
                    report.build_ms,
                    report.apply_ms,
                );
            }
        }
        WaterSimCommand::StabilizeAfterTerrainChunkChange(chunk_id) => {
            sim.stabilize_after_terrain_chunk_change(chunk_id);
        }
        WaterSimCommand::SpawnDebugParticlesAtSurface {
            surface_point_ws,
            count,
            radius_ws,
        } => {
            let result = sim.spawn_debug_particles_at_surface(surface_point_ws, count, radius_ws);
            log_water_spawn_result(surface_point_ws, result);
        }
        WaterSimCommand::Shutdown => return false,
    }
    true
}

fn water_sim_thread_sleep_duration(
    sim: &PondWaterSim,
    runtime_options: WaterSimRuntimeOptions,
    elapsed: Duration,
) -> Duration {
    let target = if !runtime_options.enabled || sim.particles.is_empty() {
        WATER_SIM_THREAD_IDLE_SLEEP
    } else {
        Duration::from_secs_f32(sim.config.substep_dt.clamp(0.001, 0.016))
    };
    target.saturating_sub(elapsed)
}

fn water_particle_frame_interval_multiplier(runtime_options: WaterSimRuntimeOptions) -> u32 {
    if runtime_options.enabled {
        WATER_SIM_COHERENT_FRAME_INTERVAL_MULTIPLIER
    } else {
        1
    }
}

fn publish_water_particle_frame(
    sim: &PondWaterSim,
    shared: &Arc<Mutex<WaterSimThreadShared>>,
    frame_revision: u64,
    worker_update_ms: f32,
    worker_substeps: u32,
    collect_perf: bool,
) -> WaterFramePublishReport {
    let total_start = collect_perf.then(Instant::now);
    let particle_count = sim.particles.len();
    let particles = sim
        .particles
        .iter()
        .map(|particle| WaterSimParticleSnapshot {
            position_ws: particle.x,
            velocity: particle.v,
        })
        .collect();
    let frame = WaterParticleFrame {
        revision: frame_revision,
        particles,
        sim_time_seconds: sim.sim_time_seconds,
        worker_update_ms,
        worker_substeps,
    };

    let lock_start = collect_perf.then(Instant::now);
    let mut guard = match shared.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let replaced_frame = guard.latest_frame.replace(frame);
    drop(guard);
    let lock_ms = lock_start
        .map(|start| start.elapsed().as_secs_f32() * 1000.0)
        .unwrap_or(0.0);
    drop(replaced_frame);
    WaterFramePublishReport {
        frame_revision,
        particle_count,
        published_particles: particle_count,
        total_ms: total_start
            .map(|start| start.elapsed().as_secs_f32() * 1000.0)
            .unwrap_or(0.0),
        lock_ms,
    }
}

fn log_water_spawn_result(center: Vec3, result: DebugWaterSpawnResult) {
    match result {
        DebugWaterSpawnResult::Spawned {
            count,
            total_particles,
        } => {
            log::info!(
                "[WATER][TOOL] spawned {} particles at ({:.3},{:.3},{:.3}) total_particles={}",
                count,
                center.x,
                center.y,
                center.z,
                total_particles,
            );
        }
        DebugWaterSpawnResult::Skipped(reason) => log_skipped_water_spawn(center, reason),
    }
}

fn log_skipped_water_spawn(center: Vec3, reason: DebugWaterSpawnSkipReason) {
    match reason {
        DebugWaterSpawnSkipReason::InvalidInput => {
            log::warn!(
                "[WATER][TOOL] skipped spawn at ({:.3},{:.3},{:.3}): invalid input",
                center.x,
                center.y,
                center.z,
            );
        }
        DebugWaterSpawnSkipReason::OutsideCurrentBounds => {
            log::debug!(
                "[WATER][TOOL] skipped spawn at ({:.3},{:.3},{:.3}): outside water bounds",
                center.x,
                center.y,
                center.z,
            );
        }
        DebugWaterSpawnSkipReason::TooCloseToBoundary { min_ws, max_ws } => {
            log::debug!(
                "[WATER][TOOL] skipped spawn at ({:.3},{:.3},{:.3}): too close to boundary accepted {:?}..{:?}",
                center.x,
                center.y,
                center.z,
                min_ws,
                max_ws,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_thread_perf_stats_accumulate_and_reset() {
        let mut stats = WaterThreadPerfStats::default();

        stats.record_tick(0.5, true);
        stats.record_tick(0.25, false);
        stats.record_command_drain(3, Duration::from_micros(500));
        stats.record_publish(WaterFramePublishReport {
            frame_revision: 7,
            particle_count: 8,
            published_particles: 2,
            total_ms: 0.25,
            lock_ms: 0.05,
        });

        assert_eq!(stats.report_seconds, 0.75);
        assert_eq!(stats.ticks, 2);
        assert_eq!(stats.active_ticks, 1);
        assert_eq!(stats.idle_ticks, 1);
        assert_eq!(stats.commands, 3);
        assert_eq!(stats.max_commands_per_tick, 3);
        assert_eq!(stats.command_drain_ms, 0.5);
        assert_eq!(stats.publish_count, 1);
        assert_eq!(stats.publish_particles, 2);
        assert_eq!(stats.publish_ms, 0.25);
        assert!((stats.publish_lock_ms - 0.05).abs() < 1.0e-6);
        assert_eq!(stats.latest_frame_revision, Some(7));

        stats.reset();
        assert_eq!(stats, WaterThreadPerfStats::default());
    }

    #[test]
    fn published_particle_frame_is_complete_and_from_one_simulation_time() {
        let mut sim = PondWaterSim::new(PondWaterConfig::default().with_particle_count(8));
        sim.sim_time_seconds = 1.25;
        for (index, particle) in sim.particles.iter_mut().enumerate() {
            particle.x = Vec3::splat(sim.sim_time_seconds + index as f32);
            particle.v = Vec3::splat(index as f32);
        }
        let shared = Arc::new(Mutex::new(WaterSimThreadShared::default()));

        let report = publish_water_particle_frame(&sim, &shared, 11, 1.0, 2, false);
        let frame = shared.lock().unwrap().latest_frame.clone().unwrap();

        assert_eq!(report.frame_revision, 11);
        assert_eq!(report.particle_count, 8);
        assert_eq!(report.published_particles, 8);
        assert_eq!(frame.revision(), 11);
        assert_eq!(frame.sim_time_seconds(), sim.sim_time_seconds);
        assert_eq!(frame.particles().len(), sim.particles.len());
        for (published, source) in frame.particles().iter().zip(&sim.particles) {
            assert_eq!(published.position_ws, source.x);
            assert_eq!(published.velocity, source.v);
        }
        assert_eq!(frame.worker_update_ms, 1.0);
        assert_eq!(frame.worker_substeps, 2);
        assert_eq!(report.total_ms, 0.0);
        assert_eq!(report.lock_ms, 0.0);
    }

    #[test]
    fn bucket_merge_never_exposes_a_mixed_time_particle_frame() {
        let mut sim = PondWaterSim::new(PondWaterConfig::default().with_particle_count(4));
        let shared = Arc::new(Mutex::new(WaterSimThreadShared::default()));

        sim.sim_time_seconds = 1.0;
        for particle in &mut sim.particles {
            particle.x = Vec3::splat(1.0);
        }
        publish_water_particle_frame(&sim, &shared, 1, 0.0, 1, false);

        sim.sim_time_seconds = 2.0;
        for particle in &mut sim.particles {
            particle.x = Vec3::splat(2.0);
        }
        publish_water_particle_frame(&sim, &shared, 2, 0.0, 1, false);

        let frame = shared.lock().unwrap().latest_frame.clone().unwrap();
        assert_eq!(frame.revision(), 2);
        assert_eq!(frame.sim_time_seconds(), 2.0);
        assert_eq!(frame.particles().len(), sim.particles.len());
        assert!(frame
            .particles()
            .iter()
            .all(|particle| particle.position_ws.x == frame.sim_time_seconds()));
    }

    #[test]
    fn coherent_frame_cadence_preserves_the_bucketed_average_copy_rate() {
        let enabled = WaterSimRuntimeOptions {
            enabled: true,
            ..WaterSimRuntimeOptions::default()
        };
        let disabled = WaterSimRuntimeOptions {
            enabled: false,
            ..enabled
        };
        let mut schedule = WaterParticleFrameSchedule::default();

        assert_eq!(water_particle_frame_interval_multiplier(enabled), 4);
        assert_eq!(
            (0..8)
                .map(|_| schedule.should_publish(enabled))
                .collect::<Vec<_>>(),
            vec![false, false, false, true, false, false, false, true]
        );
        assert_eq!(water_particle_frame_interval_multiplier(disabled), 1);
        assert!(schedule.should_publish(disabled));
        assert!(schedule.should_publish(disabled));
        assert_eq!(schedule.enabled_opportunities_since_publish, 0);
    }

    #[test]
    fn frame_consumer_retains_complete_frame_when_pending_is_missing_stale_or_busy() {
        let mut sim = AsyncWaterSim::new(PondWaterConfig::default().with_particle_count(4));
        sim.shutdown();
        sim.latest_frame = Some(test_particle_frame(4, 4.0));
        sim.shared.lock().unwrap().latest_frame = None;

        sim.poll_latest_particle_frame();
        assert_eq!(sim.latest_particle_frame().unwrap().revision(), 4);

        sim.shared.lock().unwrap().latest_frame = Some(test_particle_frame(3, 3.0));
        sim.poll_latest_particle_frame();
        assert_eq!(sim.latest_particle_frame().unwrap().revision(), 4);

        sim.shared.lock().unwrap().latest_frame = Some(test_particle_frame(5, 3.5));
        sim.poll_latest_particle_frame();
        assert_eq!(sim.latest_particle_frame().unwrap().revision(), 4);

        sim.shared.lock().unwrap().latest_frame = Some(test_particle_frame(5, 5.0));
        sim.poll_latest_particle_frame();
        let latest = sim.latest_particle_frame().unwrap();
        assert_eq!(latest.revision(), 5);
        assert_eq!(latest.sim_time_seconds(), 5.0);
        assert!(latest
            .particles()
            .iter()
            .all(|particle| particle.position_ws.x == 5.0));

        let shared = Arc::clone(&sim.shared);
        let guard = shared.lock().unwrap();
        sim.poll_latest_particle_frame();
        drop(guard);
        assert_eq!(sim.latest_particle_frame().unwrap().revision(), 5);
    }

    #[test]
    fn async_water_sim_shutdown_is_idempotent() {
        let mut sim = AsyncWaterSim::new(PondWaterConfig::default());
        sim.shutdown();
        sim.shutdown();
    }

    #[test]
    fn acknowledged_pause_disables_runtime_without_stopping_worker() {
        let mut sim = AsyncWaterSim::new(PondWaterConfig::default());
        sim.set_runtime_options(true, false, 4, Duration::from_millis(16));

        sim.pause_and_wait().unwrap();

        assert!(!sim.runtime_options.enabled);
        assert!(sim.worker.is_some());
        sim.shutdown();
    }

    fn test_particle_frame(revision: u64, sim_time_seconds: f32) -> WaterParticleFrame {
        WaterParticleFrame {
            revision,
            particles: vec![
                WaterSimParticleSnapshot {
                    position_ws: Vec3::splat(sim_time_seconds),
                    velocity: Vec3::ZERO,
                };
                4
            ],
            sim_time_seconds,
            worker_update_ms: 0.0,
            worker_substeps: 1,
        }
    }
}
