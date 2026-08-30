use super::super::{App, CHUNK_DIM};
use super::runtime::{AsyncWaterSim, WaterParticleFrame, WATER_SIM_THREAD_DEFAULT_MAX_SUBSTEPS};
use super::settings::{WaterLaunchRequest, WaterRuntimeOverrides};
use super::terrain::{
    WaterTerrainAdvanceMode, WaterTerrainAdvanceTimings, WaterTerrainRuntime, WaterTerrainStatus,
};
use crate::app::GuiAdjustables;
use crate::builder::{ContreeBuilder, PlainBuilder};
use anyhow::Result;
use glam::{UVec3, Vec3};
use re_flora_water::PondWaterConfig;
#[cfg(test)]
use std::sync::{atomic::AtomicUsize, Arc};
use std::time::Duration;

const WATER_TERRAIN_ACTIVE_MAX_SUBSTEPS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app::core) enum WaterPhase {
    Loading,
    Running,
    Quiesced,
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaterQuiescence {
    SnapshotRead,
    PublicationPending,
    PublicationFailed,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::app::core) struct WaterFrameRequest {
    pub(in crate::app::core) frame_delta_time: f32,
    pub(in crate::app::core) world_tick_seconds: f32,
    pub(in crate::app::core) world_tick_multiplier: f32,
    pub(in crate::app::core) perf_logging: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::app::core) struct WaterFrameOutcome {
    pub(in crate::app::core) advanced: bool,
    pub(in crate::app::core) max_substeps_per_tick: usize,
    pub(in crate::app::core) water_tick_seconds: f32,
}

/// Linear proof that this owner observed its publication dependencies ready and resumed.
/// Its constructor is private so persistence and App adapters cannot forge completion.
pub(in crate::app::core) struct WaterPublicationResumed {
    _private: (),
}

trait WaterPublicationTransitionHost {
    fn advance_terrain(&mut self, measure_timings: bool) -> WaterTerrainAdvanceTimings;
    fn finish_publication(&mut self) -> Option<WaterPublicationResumed>;
    fn complete_persistence(&mut self, event: WaterPublicationResumed);
}

fn advance_water_publication_transition(
    host: &mut impl WaterPublicationTransitionHost,
    measure_timings: bool,
) -> WaterTerrainAdvanceTimings {
    let timings = host.advance_terrain(measure_timings);
    if let Some(event) = host.finish_publication() {
        host.complete_persistence(event);
        log::info!("[TERRAIN_PERSISTENCE] water terrain cache Ready; water simulation resumed");
    }
    timings
}

/// Owns water configuration, simulation, terrain observation, and lifecycle as one module.
///
/// World/GPU builders remain concrete caller-provided adapters because the runtime neither owns nor
/// varies them. The ordering policy and both water workers live behind this interface.
pub(in crate::app::core) struct WaterRuntime {
    sim: AsyncWaterSim,
    terrain: WaterTerrainRuntime,
    runtime_overrides: WaterRuntimeOverrides,
    phase: WaterPhase,
    quiescence: Option<WaterQuiescence>,
}

impl WaterRuntime {
    pub(in crate::app::core) fn launch(request: WaterLaunchRequest) -> Self {
        let launch = request.resolve();
        log::info!(
            "[WATER] config profile={:?} experience={} gui_config_applied={} particles={} grid={:?} substep_dt={:.6}s terrain_margin_cells={:.2} boundary_density_min_fluid_fraction={:.2} boundary_density_max_correction={:.2} boundary_density_transition_cells={:.2} damping={:.2}/s quiet_settling={:.2}/{:.2}/s terrain_tangent_damping={:.2}/s debug_spawn_height_offset={:.2} gravity={:?} stiffness={:.1} gamma={:.2} j_min={:.3} viscosity={:.3} pressure_floor={:.3} wall_damping={:.2} collider_bounds {:?}..{:?} initial_fluid={:?} cells_per_unit={}",
            launch.profile,
            launch.experience,
            launch.gui_config_applied,
            launch.effective.particle_count,
            launch.effective.grid_dim,
            launch.effective.substep_dt,
            launch.effective.terrain_collision_margin_cells,
            launch.effective.terrain_density_min_fluid_fraction,
            launch.effective.terrain_density_max_correction_factor,
            launch.effective.terrain_density_occupancy_transition_cells,
            launch.effective.linear_damping_per_sec,
            launch.effective.quiet_settling_velocity_damping_per_sec,
            launch.effective.quiet_settling_affine_damping_per_sec,
            launch.effective.terrain_tangent_damping_per_sec,
            launch.effective.debug_spawn_height_offset,
            launch.effective.gravity,
            launch.effective.stiffness,
            launch.effective.gamma,
            launch.effective.j_min,
            launch.effective.dynamic_viscosity,
            launch.effective.pressure_floor,
            launch.effective.wall_damping,
            launch.effective.collider.min_ws,
            launch.effective.collider.max_ws,
            launch.effective.initial_fluid_bounds,
            launch.cells_per_unit,
        );
        Self {
            sim: AsyncWaterSim::new(launch.effective),
            terrain: WaterTerrainRuntime::new(),
            runtime_overrides: launch.runtime_overrides,
            phase: WaterPhase::Loading,
            quiescence: None,
        }
    }

    pub(in crate::app::core) fn phase(&self) -> WaterPhase {
        self.phase
    }

    pub(in crate::app::core) fn is_running(&self) -> bool {
        self.phase == WaterPhase::Running
    }

    pub(in crate::app::core) fn config(&self) -> &PondWaterConfig {
        &self.sim.config
    }

    pub(in crate::app::core) fn terrain_status(&self) -> WaterTerrainStatus {
        self.terrain.status()
    }

    pub(in crate::app::core) fn latest_particle_frame(&self) -> Option<&WaterParticleFrame> {
        self.sim.latest_particle_frame()
    }

    pub(in crate::app::core) fn status_text(&self, handoff_main_thread_ms: Option<f32>) -> String {
        self.sim.status_text(handoff_main_thread_ms)
    }

    pub(in crate::app::core) fn observe_visible_terrain(&mut self, chunk_dim: UVec3) {
        let bounds = self.sim.config.collider;
        self.terrain
            .observe_full_terrain(chunk_dim, bounds.min_ws, bounds.max_ws);
    }

    pub(in crate::app::core) fn advance_loading(
        &mut self,
        plain_builder: &mut PlainBuilder,
        contree_builder: &mut ContreeBuilder,
        camera_position: Vec3,
        measure_timings: bool,
    ) -> WaterTerrainAdvanceTimings {
        if self.phase != WaterPhase::Loading {
            return WaterTerrainAdvanceTimings::default();
        }
        self.terrain.advance(
            plain_builder,
            contree_builder,
            &mut self.sim,
            camera_position,
            measure_timings,
            WaterTerrainAdvanceMode::Loading,
        )
    }

    fn advance_running_terrain(
        &mut self,
        plain_builder: &mut PlainBuilder,
        contree_builder: &mut ContreeBuilder,
        camera_position: Vec3,
        measure_timings: bool,
    ) -> WaterTerrainAdvanceTimings {
        if self.phase == WaterPhase::Shutdown {
            return WaterTerrainAdvanceTimings::default();
        }
        let timings = self.terrain.advance(
            plain_builder,
            contree_builder,
            &mut self.sim,
            camera_position,
            measure_timings,
            WaterTerrainAdvanceMode::Running,
        );
        self.observe_terrain_progress();
        timings
    }

    fn observe_terrain_progress(&mut self) {
        if self.phase == WaterPhase::Loading && self.terrain.status().is_initialized() {
            self.phase = WaterPhase::Running;
        }
    }

    pub(in crate::app::core) fn advance_frame(
        &mut self,
        gui_adjustables: &GuiAdjustables,
        request: WaterFrameRequest,
    ) -> WaterFrameOutcome {
        let water_tick_seconds = crate::game_time::clamp_world_tick_seconds(
            request.world_tick_seconds * request.world_tick_multiplier.clamp(0.0, 1.0),
        );
        let max_substeps_per_tick = if self.terrain.status().has_work() {
            WATER_TERRAIN_ACTIVE_MAX_SUBSTEPS
        } else {
            WATER_SIM_THREAD_DEFAULT_MAX_SUBSTEPS
        };
        if !self.is_running() {
            return WaterFrameOutcome {
                advanced: false,
                max_substeps_per_tick,
                water_tick_seconds,
            };
        }

        self.sim
            .apply_gui_adjustables(gui_adjustables, &self.runtime_overrides);
        self.sim.set_runtime_options(
            true,
            request.perf_logging,
            max_substeps_per_tick,
            Duration::from_secs_f32(water_tick_seconds),
        );
        self.sim
            .poll_latest_particle_frame_after_frame(request.frame_delta_time, water_tick_seconds);
        WaterFrameOutcome {
            advanced: true,
            max_substeps_per_tick,
            water_tick_seconds,
        }
    }

    /// Establishes one worker-acknowledged quiescence point. Returns whether this call paused it.
    pub(in crate::app::core) fn quiesce_for_snapshot(&mut self) -> Result<bool> {
        anyhow::ensure!(
            self.phase != WaterPhase::Shutdown,
            "water runtime is already shut down"
        );
        if self.phase == WaterPhase::Quiesced {
            return Ok(false);
        }
        self.sim.pause_and_wait()?;
        self.phase = WaterPhase::Quiesced;
        self.quiescence = Some(WaterQuiescence::SnapshotRead);
        Ok(true)
    }

    pub(in crate::app::core) fn snapshot_mutation_started(&mut self) -> Result<()> {
        anyhow::ensure!(
            self.phase == WaterPhase::Quiesced
                && self.quiescence == Some(WaterQuiescence::SnapshotRead),
            "snapshot mutation requires acknowledged water quiescence"
        );
        self.quiescence = Some(WaterQuiescence::PublicationPending);
        Ok(())
    }

    pub(in crate::app::core) fn resume_after_snapshot_read(&mut self) -> bool {
        if self.phase != WaterPhase::Quiesced
            || self.quiescence != Some(WaterQuiescence::SnapshotRead)
        {
            return false;
        }
        self.quiescence = None;
        self.phase = if self.terrain.status().is_initialized() {
            WaterPhase::Running
        } else {
            WaterPhase::Loading
        };
        true
    }

    fn finish_publication_after_terrain_advance(&mut self) -> Option<WaterPublicationResumed> {
        if self.phase != WaterPhase::Quiesced
            || self.quiescence != Some(WaterQuiescence::PublicationPending)
            || !self.terrain.status().is_ready()
        {
            return None;
        }
        self.quiescence = None;
        self.phase = WaterPhase::Running;
        Some(WaterPublicationResumed { _private: () })
    }

    pub(in crate::app::core) fn retain_quiescence_after_publication_failure(&mut self) {
        if self.phase == WaterPhase::Quiesced
            && self.quiescence == Some(WaterQuiescence::PublicationPending)
        {
            self.quiescence = Some(WaterQuiescence::PublicationFailed);
        }
    }

    pub(in crate::app::core) fn shutdown(
        &mut self,
        plain_builder: &mut PlainBuilder,
    ) -> Result<()> {
        if self.phase == WaterPhase::Shutdown {
            return Ok(());
        }
        self.sim.shutdown();
        let result = self.terrain.shutdown(plain_builder);
        self.phase = WaterPhase::Shutdown;
        self.quiescence = None;
        result
    }

    #[cfg(test)]
    fn for_test(config: PondWaterConfig) -> Self {
        Self {
            sim: AsyncWaterSim::new(config),
            terrain: WaterTerrainRuntime::new(),
            runtime_overrides: WaterRuntimeOverrides::default(),
            phase: WaterPhase::Loading,
            quiescence: None,
        }
    }

    #[cfg(test)]
    fn complete_startup_for_test(&mut self) {
        self.terrain.complete_all_work_for_test();
        self.observe_terrain_progress();
    }

    #[cfg(test)]
    fn complete_publication_for_test(&mut self) {
        self.terrain.complete_all_work_for_test();
    }

    #[cfg(test)]
    fn shutdown_workers_for_test(&mut self) {
        if self.phase == WaterPhase::Shutdown {
            return;
        }
        self.stop_owned_workers();
        self.phase = WaterPhase::Shutdown;
        self.quiescence = None;
    }

    #[cfg(test)]
    fn child_workers_stopped_for_test(&self) -> bool {
        self.sim.worker_stopped_for_test() && self.terrain.workers_stopped_for_test()
    }

    #[cfg(test)]
    fn worker_exit_probes_for_test(&self) -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
        (
            self.sim.worker_exit_probe_for_test(),
            self.terrain.worker_exit_probe_for_test(),
        )
    }

    fn stop_owned_workers(&mut self) {
        self.sim.shutdown();
        self.terrain.stop_worker_threads();
    }
}

impl Drop for WaterRuntime {
    fn drop(&mut self) {
        self.stop_owned_workers();
    }
}

/// Concrete application adapter: it lends the shared world builders for one call, while the
/// runtime retains water phase, worker, and ordering ownership.
impl App {
    pub(in crate::app::core) fn enqueue_startup_water_terrain_collider_rebuilds(&mut self) {
        self.water.observe_visible_terrain(CHUNK_DIM);
    }

    pub(in crate::app::core) fn advance_water_terrain(
        &mut self,
        measure_timings: bool,
    ) -> WaterTerrainAdvanceTimings {
        let mut host = AppWaterPublicationTransition {
            water: &mut self.water,
            persistence: &mut self.terrain_persistence,
            plain_builder: &mut self.plain_builder,
            contree_builder: &mut self.contree_builder,
            camera_position: self.tracer.camera_position(),
        };
        advance_water_publication_transition(&mut host, measure_timings)
    }

    pub(in crate::app::core) fn advance_loading_water_terrain(
        &mut self,
        measure_timings: bool,
    ) -> WaterTerrainAdvanceTimings {
        self.water.advance_loading(
            &mut self.plain_builder,
            &mut self.contree_builder,
            self.tracer.camera_position(),
            measure_timings,
        )
    }
}

struct AppWaterPublicationTransition<'a> {
    water: &'a mut WaterRuntime,
    persistence: &'a mut super::super::TerrainPersistenceRuntime,
    plain_builder: &'a mut PlainBuilder,
    contree_builder: &'a mut ContreeBuilder,
    camera_position: Vec3,
}

impl WaterPublicationTransitionHost for AppWaterPublicationTransition<'_> {
    fn advance_terrain(&mut self, measure_timings: bool) -> WaterTerrainAdvanceTimings {
        self.water.advance_running_terrain(
            self.plain_builder,
            self.contree_builder,
            self.camera_position,
            measure_timings,
        )
    }

    fn finish_publication(&mut self) -> Option<WaterPublicationResumed> {
        self.water.finish_publication_after_terrain_advance()
    }

    fn complete_persistence(&mut self, event: WaterPublicationResumed) {
        self.persistence.complete_published_load(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::GuiAdjustables;
    use glam::UVec3;
    use re_flora_water::PondWaterConfig;
    use std::sync::atomic::Ordering;

    fn running_runtime() -> WaterRuntime {
        let mut runtime = WaterRuntime::for_test(PondWaterConfig::default());
        runtime.complete_startup_for_test();
        assert_eq!(runtime.phase(), WaterPhase::Running);
        runtime
    }

    #[test]
    fn loading_becomes_running_only_after_terrain_is_initialized() {
        let mut runtime = WaterRuntime::for_test(PondWaterConfig::default());
        assert_eq!(runtime.phase(), WaterPhase::Loading);

        runtime.observe_terrain_progress();
        assert_eq!(runtime.phase(), WaterPhase::Loading);

        runtime.complete_startup_for_test();
        assert_eq!(runtime.phase(), WaterPhase::Running);
    }

    #[test]
    fn quiesce_is_acknowledged_and_idempotent() {
        let mut runtime = running_runtime();

        assert!(runtime.quiesce_for_snapshot().unwrap());
        assert!(!runtime.quiesce_for_snapshot().unwrap());
        assert_eq!(runtime.phase(), WaterPhase::Quiesced);
        runtime.shutdown_workers_for_test();
    }

    #[test]
    fn snapshot_read_without_mutation_releases_quiescence_once() {
        let mut runtime = running_runtime();
        runtime.quiesce_for_snapshot().unwrap();

        assert!(runtime.resume_after_snapshot_read());
        assert!(!runtime.resume_after_snapshot_read());
        assert_eq!(runtime.phase(), WaterPhase::Running);
        runtime.shutdown_workers_for_test();
    }

    #[test]
    fn snapshot_mutation_resumes_only_after_terrain_publication_is_ready() {
        let mut runtime = running_runtime();
        runtime.quiesce_for_snapshot().unwrap();
        runtime.snapshot_mutation_started().unwrap();
        runtime.observe_visible_terrain(UVec3::ONE);

        assert!(runtime.finish_publication_after_terrain_advance().is_none());
        assert_eq!(runtime.phase(), WaterPhase::Quiesced);

        runtime.complete_publication_for_test();
        let resumed = runtime
            .finish_publication_after_terrain_advance()
            .expect("ready owner must emit one linear resume event");
        assert_eq!(runtime.phase(), WaterPhase::Running);
        assert!(runtime.finish_publication_after_terrain_advance().is_none());
        drop(resumed);
        runtime.shutdown_workers_for_test();
    }

    #[test]
    fn failed_publication_after_mutation_stays_quiesced() {
        let mut runtime = running_runtime();
        runtime.quiesce_for_snapshot().unwrap();
        runtime.snapshot_mutation_started().unwrap();

        runtime.retain_quiescence_after_publication_failure();

        assert!(runtime.finish_publication_after_terrain_advance().is_none());
        assert_eq!(runtime.phase(), WaterPhase::Quiesced);
        runtime.shutdown_workers_for_test();
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum RecordedPublicationAction {
        AdvanceTerrain,
        FinishPublication,
        CompletePersistence,
    }

    struct RecordingPublicationHost {
        runtime: WaterRuntime,
        persistence: super::super::super::TerrainPersistenceRuntime,
        actions: Vec<RecordedPublicationAction>,
    }

    impl WaterPublicationTransitionHost for RecordingPublicationHost {
        fn advance_terrain(&mut self, _measure_timings: bool) -> WaterTerrainAdvanceTimings {
            self.actions.push(RecordedPublicationAction::AdvanceTerrain);
            self.runtime.complete_publication_for_test();
            WaterTerrainAdvanceTimings::default()
        }

        fn finish_publication(&mut self) -> Option<WaterPublicationResumed> {
            self.actions
                .push(RecordedPublicationAction::FinishPublication);
            self.runtime.finish_publication_after_terrain_advance()
        }

        fn complete_persistence(&mut self, event: WaterPublicationResumed) {
            self.actions
                .push(RecordedPublicationAction::CompletePersistence);
            self.persistence.complete_published_load(event);
        }
    }

    #[test]
    fn production_transition_advances_owner_then_consumes_its_linear_resume_event() {
        let mut runtime = running_runtime();
        runtime.quiesce_for_snapshot().unwrap();
        runtime.snapshot_mutation_started().unwrap();
        let mut host = RecordingPublicationHost {
            runtime,
            persistence:
                super::super::super::TerrainPersistenceRuntime::published_awaiting_dependents_for_test(
                ),
            actions: Vec::new(),
        };

        advance_water_publication_transition(&mut host, false);

        assert_eq!(
            host.actions,
            vec![
                RecordedPublicationAction::AdvanceTerrain,
                RecordedPublicationAction::FinishPublication,
                RecordedPublicationAction::CompletePersistence,
            ]
        );
        assert!(host.persistence.can_start_operation());
        assert_eq!(host.runtime.phase(), WaterPhase::Running);
        host.runtime.shutdown_workers_for_test();
    }

    #[test]
    fn ordinary_frame_limits_substeps_while_terrain_work_is_active() {
        let mut runtime = running_runtime();
        runtime.observe_visible_terrain(UVec3::ONE);

        let outcome = runtime.advance_frame(
            &GuiAdjustables::default(),
            WaterFrameRequest {
                frame_delta_time: 1.0 / 60.0,
                world_tick_seconds: 1.0 / 30.0,
                world_tick_multiplier: 0.5,
                perf_logging: false,
            },
        );

        assert!(outcome.advanced);
        assert_eq!(outcome.max_substeps_per_tick, 2);
        assert_eq!(outcome.water_tick_seconds, 1.0 / 60.0);
        runtime.shutdown_workers_for_test();
    }

    #[test]
    fn shutdown_is_idempotent_and_consumes_both_child_workers() {
        let mut runtime = running_runtime();
        let (sim_exits, terrain_exits) = runtime.worker_exit_probes_for_test();

        runtime.shutdown_workers_for_test();
        runtime.shutdown_workers_for_test();

        assert_eq!(runtime.phase(), WaterPhase::Shutdown);
        assert!(runtime.child_workers_stopped_for_test());
        assert_eq!(sim_exits.load(Ordering::SeqCst), 1);
        assert_eq!(terrain_exits.load(Ordering::SeqCst), 2);
        drop(runtime);
        assert_eq!(sim_exits.load(Ordering::SeqCst), 1);
        assert_eq!(terrain_exits.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn fallback_drop_joins_every_owned_worker() {
        let runtime = running_runtime();
        let (sim_exits, terrain_exits) = runtime.worker_exit_probes_for_test();

        drop(runtime);

        assert_eq!(sim_exits.load(Ordering::SeqCst), 1);
        assert_eq!(terrain_exits.load(Ordering::SeqCst), 2);
    }
}
