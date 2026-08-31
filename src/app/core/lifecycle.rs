use super::App;
use anyhow::{anyhow, Context, Result};
use std::fmt;
use winit::event_loop::ActiveEventLoop;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum ShutdownState {
    #[default]
    Running,
    Terminating,
    Terminated(ShutdownReport),
}

#[derive(Debug, Default)]
pub(super) struct AppShutdownLifecycle {
    state: ShutdownState,
}

trait ShutdownActions {
    fn quiesce_producers(&mut self);
    fn shutdown_water(&mut self) -> Result<()>;
    fn discard_contree_readback(&mut self) -> Result<()>;
    fn join_dependent_workers(&mut self);
    fn shutdown_audio(&mut self) -> Result<()>;
    fn wait_device_idle(&mut self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShutdownPhase {
    Transaction,
    Water,
    ContreeReadback,
    Audio,
}

impl fmt::Display for ShutdownPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transaction => formatter.write_str("transaction"),
            Self::Water => formatter.write_str("water"),
            Self::ContreeReadback => formatter.write_str("Contree readback"),
            Self::Audio => formatter.write_str("audio"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShutdownFailure {
    phase: ShutdownPhase,
    detail: String,
}

impl ShutdownFailure {
    fn new(phase: ShutdownPhase, error: anyhow::Error) -> Self {
        Self {
            phase,
            detail: format!("{error:#}"),
        }
    }

    #[cfg(test)]
    fn phase(&self) -> ShutdownPhase {
        self.phase
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ShutdownReport {
    failures: Vec<ShutdownFailure>,
}

impl ShutdownReport {
    fn interrupted() -> Self {
        Self {
            failures: vec![ShutdownFailure {
                phase: ShutdownPhase::Transaction,
                detail: "a prior shutdown transaction was interrupted; one-shot owners were not invoked again"
                    .into(),
            }],
        }
    }

    fn record(&mut self, phase: ShutdownPhase, result: Result<()>) {
        if let Err(error) = result {
            self.failures.push(ShutdownFailure::new(phase, error));
        }
    }

    fn is_success(&self) -> bool {
        self.failures.is_empty()
    }

    fn result(&self) -> Result<()> {
        if self.is_success() {
            Ok(())
        } else {
            Err(anyhow!("application shutdown failed: {self}"))
        }
    }

    #[cfg(test)]
    fn failures(&self) -> &[ShutdownFailure] {
        &self.failures
    }
}

impl fmt::Display for ShutdownReport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, failure) in self.failures.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; ")?;
            }
            write!(formatter, "{}: {}", failure.phase, failure.detail)?;
        }
        Ok(())
    }
}

impl AppShutdownLifecycle {
    pub(super) fn is_started(&self) -> bool {
        !matches!(self.state, ShutdownState::Running)
    }

    fn interrupted() -> Self {
        Self {
            state: ShutdownState::Terminated(ShutdownReport::interrupted()),
        }
    }

    fn report(&self) -> Option<&ShutdownReport> {
        match &self.state {
            ShutdownState::Terminated(report) => Some(report),
            ShutdownState::Running | ShutdownState::Terminating => None,
        }
    }

    fn terminate(&mut self, actions: &mut impl ShutdownActions) -> ShutdownReport {
        if matches!(self.state, ShutdownState::Terminating) {
            self.state = ShutdownState::Terminated(ShutdownReport::interrupted());
        }
        if let ShutdownState::Terminated(report) = &self.state {
            return report.clone();
        }
        self.state = ShutdownState::Terminating;

        log::info!("[SHUTDOWN] phase=quiesce_producers");
        actions.quiesce_producers();

        let mut report = ShutdownReport::default();
        log::info!("[SHUTDOWN] phase=consume_managed_gpu_jobs");
        report.record(ShutdownPhase::Water, actions.shutdown_water());
        report.record(
            ShutdownPhase::ContreeReadback,
            actions.discard_contree_readback(),
        );

        log::info!("[SHUTDOWN] phase=join_dependent_workers");
        actions.join_dependent_workers();

        log::info!("[SHUTDOWN] phase=shutdown_audio");
        report.record(ShutdownPhase::Audio, actions.shutdown_audio());

        // This is the one device-wide idle boundary for terminal App teardown. It runs after every
        // owner was asked to quiesce, even when an earlier one-shot owner reported a failure.
        log::info!("[SHUTDOWN] phase=wait_device_idle");
        actions.wait_device_idle();

        log::info!(
            "[SHUTDOWN] phase=complete failures={}",
            report.failures.len()
        );
        self.state = ShutdownState::Terminated(report);
        self.report()
            .expect("a completed shutdown transaction must retain its report")
            .clone()
    }

    #[cfg(test)]
    fn is_terminated(&self) -> bool {
        matches!(self.state, ShutdownState::Terminated(_))
    }
}

impl ShutdownActions for App {
    fn quiesce_producers(&mut self) {
        self.stop_terrain_edit_loop_sound();
        self.abort_loading_visible_terrain_publication();
    }

    fn shutdown_water(&mut self) -> Result<()> {
        self.water
            .shutdown(&mut self.plain_builder)
            .context("shut down water runtime")
    }

    fn discard_contree_readback(&mut self) -> Result<()> {
        self.contree_builder
            .discard_active_cpu_chunk_cache_job()
            .context("discard active Contree CPU-cache GPU readback")
    }

    fn join_dependent_workers(&mut self) {
        if let Some(runtime) = self.emissive_voxel_lighting.as_mut() {
            runtime.shutdown();
        }
        self.contree_builder.shutdown_cpu_chunk_cache_worker();
    }

    fn shutdown_audio(&mut self) -> Result<()> {
        self.spatial_sound_manager
            .stop()
            .context("shut down audio runtime")
    }

    fn wait_device_idle(&mut self) {
        self.vulkan_ctx.device().wait_idle();
    }
}

impl App {
    pub fn on_terminate(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(err) = self.shutdown_for_termination() {
            log::error!("[SHUTDOWN] terminal transaction completed with failures: {err:#}");
        }
        event_loop.exit();
    }

    pub fn on_about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.shutdown_lifecycle.is_started() {
            return;
        }
        let accepted_extent = self.resize_lifecycle_test.as_mut().and_then(|test| {
            test.request_next(
                &self.window_state.window(),
                self.time_info.total_frame_count(),
            )
        });
        if let Some(accepted_extent) = accepted_extent {
            self.queue_frame_extent(accepted_extent);
        }
        if !self.window_state.is_minimized() {
            self.window_state.window().request_redraw();
        }
    }

    /// Drives the one legal application teardown transition.
    ///
    /// Every real owner is consumed at most once. Failures are retained in the terminal report, so
    /// close requests, keyboard exit, auto-exit, and `Drop` all observe the same result without
    /// retrying one-shot water, readback, worker, or audio capabilities.
    pub(super) fn shutdown_for_termination(&mut self) -> Result<()> {
        if let Some(report) = self.shutdown_lifecycle.report() {
            return report.result();
        }

        // Leave an auditable terminal sentinel in `App` while the lifecycle is lent the complete
        // production adapter. If a phase panics, `Drop` will not re-invoke already consumed owners.
        let mut lifecycle = std::mem::replace(
            &mut self.shutdown_lifecycle,
            AppShutdownLifecycle::interrupted(),
        );
        let report = lifecycle.terminate(self);
        self.shutdown_lifecycle = lifecycle;
        report.result()
    }

    pub(super) fn queue_frame_extent(&mut self, extent: re_flora_vkn::Extent2D) {
        self.pending_frame_extent = Some(extent);
    }

    pub(super) fn queue_current_frame_extent(&mut self) {
        self.queue_frame_extent(self.window_state.window_extent());
    }

    pub(super) fn on_resize(&mut self) {
        let Some(requested_extent) = self.pending_frame_extent.take() else {
            return;
        };
        if requested_extent.width == 0 || requested_extent.height == 0 {
            self.pending_frame_extent = Some(requested_extent);
            return;
        }

        self.frame_manager.wait_for_all_submissions();
        // A frame-submit fence covers command execution, but the presentation operation can still
        // be consuming the render-finished semaphore when the fence is observed.  Wait only the
        // presentation queue before destroying swapchain-owned semaphores/images; this preserves
        // the no-device-wide-idle resize contract while satisfying Vulkan lifetime rules.
        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        let frame_extent_generation = self.swapchain.on_resize(requested_extent);
        self.frame_manager
            .recreate_swapchain_images(self.vulkan_ctx.device(), self.swapchain.image_count());
        self.tracer.on_resize(
            frame_extent_generation,
            self.contree_builder.get_resources(),
            self.scene_accel_builder.get_resources(),
            self.plain_builder.get_resources(),
        );

        self.egui_renderer
            .set_render_pass(self.swapchain.get_render_pass());

        if let Some(test) = self.resize_lifecycle_test.as_mut() {
            test.observe(
                frame_extent_generation.extent(),
                frame_extent_generation.serial(),
            );
        }
        log::info!(
            "[RESIZE] published generation={} extent={}x{} tracer_generation={}",
            frame_extent_generation.serial(),
            frame_extent_generation.extent().width,
            frame_extent_generation.extent().height,
            self.tracer.frame_extent_generation().serial(),
        );
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::{AppShutdownLifecycle, ShutdownActions, ShutdownPhase};
    use anyhow::{anyhow, Result};

    struct RecordingShutdownActions {
        calls: Vec<&'static str>,
        water_result: Option<Result<()>>,
        discard_result: Option<Result<()>>,
        audio_result: Option<Result<()>>,
    }

    impl RecordingShutdownActions {
        fn successful() -> Self {
            Self {
                calls: Vec::new(),
                water_result: Some(Ok(())),
                discard_result: Some(Ok(())),
                audio_result: Some(Ok(())),
            }
        }
    }

    impl ShutdownActions for RecordingShutdownActions {
        fn quiesce_producers(&mut self) {
            self.calls.push("quiesce");
        }

        fn shutdown_water(&mut self) -> Result<()> {
            self.calls.push("water");
            self.water_result
                .take()
                .expect("water owner cannot be consumed twice")
        }

        fn discard_contree_readback(&mut self) -> Result<()> {
            self.calls.push("discard");
            self.discard_result
                .take()
                .expect("Contree readback owner cannot be consumed twice")
        }

        fn join_dependent_workers(&mut self) {
            self.calls.push("join");
        }

        fn shutdown_audio(&mut self) -> Result<()> {
            self.calls.push("audio");
            self.audio_result
                .take()
                .expect("audio owner cannot be consumed twice")
        }

        fn wait_device_idle(&mut self) {
            self.calls.push("idle");
        }
    }

    #[test]
    fn terminal_shutdown_attempts_every_owner_once_and_persists_failures() {
        let mut lifecycle = AppShutdownLifecycle::default();
        let mut actions = RecordingShutdownActions {
            calls: Vec::new(),
            water_result: Some(Err(anyhow!("water owner was consumed"))),
            discard_result: Some(Ok(())),
            audio_result: Some(Err(anyhow!("audio owner was consumed"))),
        };

        let first = lifecycle.terminate(&mut actions);

        assert_eq!(
            actions.calls,
            ["quiesce", "water", "discard", "join", "audio", "idle"]
        );
        assert_eq!(
            first
                .failures()
                .iter()
                .map(|failure| failure.phase())
                .collect::<Vec<_>>(),
            [ShutdownPhase::Water, ShutdownPhase::Audio]
        );
        assert!(first.to_string().contains("water owner was consumed"));
        assert!(first.to_string().contains("audio owner was consumed"));
        assert!(lifecycle.is_terminated());

        let second = lifecycle.terminate(&mut actions);
        assert_eq!(second, first);
        assert_eq!(
            actions.calls,
            ["quiesce", "water", "discard", "join", "audio", "idle"]
        );
    }

    #[test]
    fn successful_terminal_shutdown_reentry_is_a_noop() {
        let mut lifecycle = AppShutdownLifecycle::default();
        let mut actions = RecordingShutdownActions::successful();

        assert!(lifecycle.terminate(&mut actions).is_success());
        assert!(lifecycle.terminate(&mut actions).is_success());
        assert_eq!(
            actions.calls,
            ["quiesce", "water", "discard", "join", "audio", "idle"]
        );
    }
}
