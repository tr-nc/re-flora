use super::apply_water_gui_adjustables_to_config;
use crate::app::GuiAdjustables;
use glam::{IVec3, Vec3};
use re_flora_water::{
    DebugWaterSpawnResult, DebugWaterSpawnSkipReason, PondWaterConfig, PondWaterSim,
    WaterTerrainCacheBuildRequest, WaterTerrainCachePatch, WaterTerrainColliderChunk,
    WaterTerrainColliderSet,
};
use std::{
    sync::{mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(crate) const WATER_SIM_THREAD_DEFAULT_MAX_SUBSTEPS: usize = 4;
const WATER_SIM_COMMAND_CHANNEL_CAPACITY: usize = 256;
const WATER_SIM_THREAD_MAX_COMMANDS_PER_TICK: usize = 1024;
const WATER_SIM_THREAD_SNAPSHOT_INTERVAL: Duration = Duration::from_millis(16);
const WATER_SIM_THREAD_IDLE_SLEEP: Duration = Duration::from_millis(16);
const WATER_SIM_SNAPSHOT_BUCKET_COUNT: usize = 4;

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

enum WaterSimCommand {
    UpdateConfig(PondWaterConfig),
    SetRuntimeOptions(WaterSimRuntimeOptions),
    UpsertTerrainColliderChunkDeferred(WaterTerrainColliderChunk),
    RemoveTerrainColliderChunkDeferred(IVec3),
    FinishTerrainColliderChunkBatch {
        stabilize_particles: bool,
    },
    InvalidateTerrainGridCacheForChunk(IVec3),
    ApplyTerrainGridCachePatch(WaterTerrainCachePatch),
    StabilizeAfterTerrainChunkChange(IVec3),
    SpawnDebugParticlesAtSurface {
        surface_point_ws: Vec3,
        count: usize,
        radius_ws: f32,
    },
    Shutdown,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WaterSimParticleSnapshot {
    pub(crate) position_ws: Vec3,
    pub(crate) velocity: Vec3,
}

impl WaterSimParticleSnapshot {
    fn invisible() -> Self {
        Self {
            position_ws: Vec3::splat(f32::NAN),
            velocity: Vec3::ZERO,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct WaterSimSnapshot {
    pub(crate) particles: Vec<WaterSimParticleSnapshot>,
    particle_bounds_ws: Option<(Vec3, Vec3)>,
    particle_count: usize,
    sim_time_seconds: f32,
    worker_update_ms: f32,
    worker_substeps: u32,
    publish_bucket_index: usize,
    publish_bucket_count: usize,
}

#[derive(Default)]
struct WaterSimThreadShared {
    latest_snapshot: Option<WaterSimSnapshot>,
}

pub(crate) struct AsyncWaterSim {
    pub(crate) config: PondWaterConfig,
    pub(crate) dx: f32,
    terrain: Option<WaterTerrainColliderSet>,
    command_tx: mpsc::SyncSender<WaterSimCommand>,
    shared: Arc<Mutex<WaterSimThreadShared>>,
    worker: Option<JoinHandle<()>>,
    latest_snapshot: Option<WaterSimSnapshot>,
    snapshot_poll_accumulator: f32,
    last_sent_config: PondWaterConfig,
    runtime_options: WaterSimRuntimeOptions,
}

impl AsyncWaterSim {
    pub(crate) fn new(config: PondWaterConfig) -> Self {
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
            latest_snapshot: None,
            snapshot_poll_accumulator: 0.0,
            last_sent_config: config,
            runtime_options: WaterSimRuntimeOptions::default(),
        }
    }

    pub(crate) fn apply_gui_adjustables(&mut self, gui_adjustables: &GuiAdjustables) {
        apply_water_gui_adjustables_to_config(&mut self.config, gui_adjustables);
        self.dx = water_config_dx(&self.config);
        if self.config != self.last_sent_config {
            let config = self.config.clone();
            if self.try_send_coalescable_command(WaterSimCommand::UpdateConfig(config.clone())) {
                self.last_sent_config = config;
            }
        }
    }

    pub(crate) fn set_runtime_options(
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

    pub(crate) fn poll_latest_snapshot_after_frame(
        &mut self,
        frame_delta_time: f32,
        water_tick_seconds: f32,
    ) {
        self.snapshot_poll_accumulator += frame_delta_time.max(0.0);
        let water_tick_elapsed = self.snapshot_poll_accumulator >= water_tick_seconds;
        if water_tick_elapsed {
            self.snapshot_poll_accumulator %= water_tick_seconds;
        }
        if water_tick_elapsed || self.latest_snapshot.is_none() {
            self.poll_latest_snapshot();
        }
    }

    fn poll_latest_snapshot(&mut self) {
        let latest = match self.shared.lock() {
            Ok(mut guard) => guard.latest_snapshot.take(),
            Err(poisoned) => poisoned.into_inner().latest_snapshot.take(),
        };
        if let Some(snapshot) = latest {
            self.merge_latest_snapshot(snapshot);
        }
    }

    fn merge_latest_snapshot(&mut self, snapshot: WaterSimSnapshot) {
        if snapshot.publish_bucket_count <= 1 {
            self.latest_snapshot = Some(snapshot);
            return;
        }

        let publish_bucket_index =
            snapshot.publish_bucket_index % snapshot.publish_bucket_count.max(1);
        let publish_bucket_count = snapshot.publish_bucket_count;
        let mut display_snapshot =
            self.latest_snapshot
                .take()
                .unwrap_or_else(|| WaterSimSnapshot {
                    particles: vec![WaterSimParticleSnapshot::invisible(); snapshot.particle_count],
                    particle_bounds_ws: snapshot.particle_bounds_ws,
                    particle_count: snapshot.particle_count,
                    sim_time_seconds: snapshot.sim_time_seconds,
                    worker_update_ms: snapshot.worker_update_ms,
                    worker_substeps: snapshot.worker_substeps,
                    publish_bucket_index,
                    publish_bucket_count,
                });

        display_snapshot.particles.resize(
            snapshot.particle_count,
            WaterSimParticleSnapshot::invisible(),
        );
        for (bucket_slot, particle) in snapshot.particles.iter().copied().enumerate() {
            let particle_index = publish_bucket_index + bucket_slot * publish_bucket_count;
            if let Some(display_particle) = display_snapshot.particles.get_mut(particle_index) {
                *display_particle = particle;
            }
        }
        display_snapshot.particle_bounds_ws = snapshot.particle_bounds_ws;
        display_snapshot.particle_count = snapshot.particle_count;
        display_snapshot.sim_time_seconds = snapshot.sim_time_seconds;
        display_snapshot.worker_update_ms = snapshot.worker_update_ms;
        display_snapshot.worker_substeps = snapshot.worker_substeps;
        display_snapshot.publish_bucket_index = publish_bucket_index;
        display_snapshot.publish_bucket_count = publish_bucket_count;
        self.latest_snapshot = Some(display_snapshot);
    }

    pub(crate) fn latest_particles(&self) -> &[WaterSimParticleSnapshot] {
        self.latest_snapshot
            .as_ref()
            .map_or(&[], |snapshot| snapshot.particles.as_slice())
    }

    pub(crate) fn particle_bounds_ws(&self) -> Option<(Vec3, Vec3)> {
        self.latest_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.particle_bounds_ws)
    }

    pub(crate) fn status_text(&self, handoff_main_thread_ms: Option<f32>) -> String {
        let Some(snapshot) = self.latest_snapshot.as_ref() else {
            return "Water sim thread: --".to_owned();
        };
        match handoff_main_thread_ms {
            Some(handoff_ms) => format!(
                "Water sim thread: handoff {:.3} ms, worker {:.3} ms, substeps {}, particles {}, sim {:.2}s",
                handoff_ms,
                snapshot.worker_update_ms,
                snapshot.worker_substeps,
                snapshot.particle_count,
                snapshot.sim_time_seconds,
            ),
            None => format!(
                "Water sim thread: worker {:.3} ms, substeps {}, particles {}, sim {:.2}s",
                snapshot.worker_update_ms,
                snapshot.worker_substeps,
                snapshot.particle_count,
                snapshot.sim_time_seconds,
            ),
        }
    }

    pub(crate) fn terrain_collider_set(&self) -> Option<&WaterTerrainColliderSet> {
        self.terrain.as_ref()
    }

    pub(crate) fn terrain_grid_cache_build_request_for_chunk(
        &self,
        chunk_id: IVec3,
    ) -> Option<WaterTerrainCacheBuildRequest> {
        WaterTerrainCacheBuildRequest::for_config_and_terrain(
            &self.config,
            self.terrain.clone(),
            chunk_id,
        )
    }

    pub(crate) fn upsert_terrain_collider_chunk_deferred(
        &mut self,
        chunk: WaterTerrainColliderChunk,
    ) {
        self.terrain
            .get_or_insert_with(WaterTerrainColliderSet::new)
            .insert_chunk(Arc::new(chunk.clone()));
        self.send_critical_command(WaterSimCommand::UpsertTerrainColliderChunkDeferred(chunk));
    }

    pub(crate) fn remove_terrain_collider_chunk_deferred(&mut self, chunk_id: IVec3) -> bool {
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

    pub(crate) fn finish_terrain_collider_chunk_batch(&mut self, stabilize_particles: bool) {
        self.send_critical_command(WaterSimCommand::FinishTerrainColliderChunkBatch {
            stabilize_particles,
        });
    }

    pub(crate) fn invalidate_terrain_grid_cache_for_chunk(&mut self, chunk_id: IVec3) {
        self.send_critical_command(WaterSimCommand::InvalidateTerrainGridCacheForChunk(
            chunk_id,
        ));
    }

    pub(crate) fn submit_terrain_grid_cache_patch(&mut self, patch: WaterTerrainCachePatch) {
        self.send_critical_command(WaterSimCommand::ApplyTerrainGridCachePatch(patch));
    }

    pub(crate) fn stabilize_after_terrain_chunk_change(&mut self, chunk_id: IVec3) {
        self.send_critical_command(WaterSimCommand::StabilizeAfterTerrainChunkChange(chunk_id));
    }

    pub(crate) fn request_debug_particle_spawn(
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
}

impl Drop for AsyncWaterSim {
    fn drop(&mut self) {
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
    let mut publish_bucket_index = 0usize;
    publish_water_sim_snapshot(&sim, &shared, 0.0, 0, 0, 1);

    loop {
        let tick_start = Instant::now();
        for _ in 0..WATER_SIM_THREAD_MAX_COMMANDS_PER_TICK {
            match command_rx.try_recv() {
                Ok(command) => {
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

        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;
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
            let publish_bucket_count = if runtime_options.enabled {
                WATER_SIM_SNAPSHOT_BUCKET_COUNT
            } else {
                1
            };
            publish_water_sim_snapshot(
                &sim,
                &shared,
                worker_update_ms,
                worker_substeps,
                publish_bucket_index,
                publish_bucket_count,
            );
            publish_bucket_index = if publish_bucket_count > 1 {
                (publish_bucket_index + 1) % publish_bucket_count
            } else {
                0
            };
            last_publish = Instant::now();
        }

        let sleep_for =
            water_sim_thread_sleep_duration(&sim, runtime_options, tick_start.elapsed());
        match command_rx.recv_timeout(sleep_for) {
            Ok(command) => {
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

fn publish_water_sim_snapshot(
    sim: &PondWaterSim,
    shared: &Arc<Mutex<WaterSimThreadShared>>,
    worker_update_ms: f32,
    worker_substeps: u32,
    publish_bucket_index: usize,
    publish_bucket_count: usize,
) {
    let particle_count = sim.particles.len();
    let publish_bucket_count = publish_bucket_count.max(1);
    let publish_bucket_index = publish_bucket_index % publish_bucket_count;
    let publish_all_particles = publish_bucket_count <= 1;
    let publish_count = if publish_all_particles {
        particle_count
    } else {
        particle_count.saturating_add(publish_bucket_count - 1 - publish_bucket_index)
            / publish_bucket_count
    };

    let mut min_ws = Vec3::splat(f32::INFINITY);
    let mut max_ws = Vec3::splat(f32::NEG_INFINITY);
    let mut particles = Vec::with_capacity(publish_count);
    for (particle_index, particle) in sim.particles.iter().enumerate() {
        if publish_all_particles || particle_index % publish_bucket_count == publish_bucket_index {
            particles.push(WaterSimParticleSnapshot {
                position_ws: particle.x,
                velocity: particle.v,
            });
        }
        if particle.x.is_finite() {
            min_ws = min_ws.min(particle.x);
            max_ws = max_ws.max(particle.x);
        }
    }
    let particle_bounds_ws = min_ws.is_finite().then_some((min_ws, max_ws));
    let snapshot = WaterSimSnapshot {
        particle_count,
        particles,
        particle_bounds_ws,
        sim_time_seconds: sim.sim_time_seconds,
        worker_update_ms,
        worker_substeps,
        publish_bucket_index,
        publish_bucket_count,
    };

    match shared.lock() {
        Ok(mut guard) => {
            guard.latest_snapshot = Some(snapshot);
        }
        Err(poisoned) => {
            poisoned.into_inner().latest_snapshot = Some(snapshot);
        }
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
