use super::{emissive_voxel_lighting, water, App, ContreeBuilder, PlainBuilder, VulkanContext};
use anyhow::{anyhow, Context, Result};
use winit::event_loop::ActiveEventLoop;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ShutdownState {
    #[default]
    Running,
    Quiescing,
    Complete,
}

#[derive(Debug, Default)]
pub(super) struct AppShutdownLifecycle {
    state: ShutdownState,
    water_complete: bool,
    contree_readback_complete: bool,
    workers_joined: bool,
}

trait ShutdownActions {
    fn shutdown_water(&mut self) -> Result<()>;
    fn discard_contree_readback(&mut self) -> Result<()>;
    fn join_dependent_workers(&mut self);
    fn wait_device_idle(&mut self);
}

impl AppShutdownLifecycle {
    pub(super) fn is_started(&self) -> bool {
        self.state != ShutdownState::Running
    }

    pub(super) fn is_complete(&self) -> bool {
        self.state == ShutdownState::Complete
    }

    fn begin(&mut self) -> bool {
        if self.state != ShutdownState::Running {
            return false;
        }
        self.state = ShutdownState::Quiescing;
        true
    }

    #[cfg(test)]
    fn state(&self) -> ShutdownState {
        self.state
    }

    fn drain(&mut self, actions: &mut impl ShutdownActions) -> Result<()> {
        if self.state == ShutdownState::Complete {
            return Ok(());
        }
        self.begin();

        let mut failures = Vec::new();
        if !self.water_complete || !self.contree_readback_complete {
            log::info!("[SHUTDOWN] phase=consume_managed_gpu_jobs");
        }
        if !self.water_complete {
            match actions.shutdown_water() {
                Ok(()) => self.water_complete = true,
                Err(error) => failures.push(error),
            }
        }
        if !self.contree_readback_complete {
            match actions.discard_contree_readback() {
                Ok(()) => self.contree_readback_complete = true,
                Err(error) => failures.push(error),
            }
        }
        if !self.workers_joined {
            log::info!("[SHUTDOWN] phase=join_dependent_workers");
            actions.join_dependent_workers();
            self.workers_joined = true;
        }

        // Every attempt ends behind a device-idle boundary. A retry can successfully consume GPU
        // work that a prior failed phase retained, so it must establish a fresh boundary too.
        log::info!("[SHUTDOWN] phase=wait_device_idle");
        actions.wait_device_idle();

        if failures.is_empty() {
            debug_assert!(
                self.water_complete && self.contree_readback_complete && self.workers_joined
            );
            self.state = ShutdownState::Complete;
            log::info!("[SHUTDOWN] phase=complete");
            return Ok(());
        }

        let details = failures
            .iter()
            .map(|failure| format!("{failure:#}"))
            .collect::<Vec<_>>()
            .join("; ");
        Err(anyhow!("shutdown phases failed: {details}"))
    }
}

struct AppShutdownActions<'a> {
    water: &'a mut water::WaterRuntime,
    plain_builder: &'a mut PlainBuilder,
    contree_builder: &'a mut ContreeBuilder,
    emissive_voxel_lighting: &'a mut Option<emissive_voxel_lighting::EmissiveVoxelLightingRuntime>,
    vulkan_ctx: &'a VulkanContext,
}

impl ShutdownActions for AppShutdownActions<'_> {
    fn shutdown_water(&mut self) -> Result<()> {
        self.water
            .shutdown(self.plain_builder)
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

    fn wait_device_idle(&mut self) {
        self.vulkan_ctx.device().wait_idle();
    }
}

impl App {
    pub fn on_terminate(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(err) = self.shutdown_for_termination() {
            log::error!("[SHUTDOWN] failed to drain application GPU work: {err:#}");
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

    /// Drives the one legal application teardown transition. Quiescing is published before any
    /// blocking operation; successful phases are retained across retries, while every phase after
    /// a failure is still attempted so no worker is abandoned behind an early return.
    pub(super) fn shutdown_for_termination(&mut self) -> Result<()> {
        if self.shutdown_lifecycle.is_complete() {
            return Ok(());
        }

        if self.shutdown_lifecycle.begin() {
            log::info!("[SHUTDOWN] phase=quiesce_producers");
            self.stop_terrain_edit_loop_sound();
            self.abort_loading_visible_terrain_publication();
        }

        let mut actions = AppShutdownActions {
            water: &mut self.water,
            plain_builder: &mut self.plain_builder,
            contree_builder: &mut self.contree_builder,
            emissive_voxel_lighting: &mut self.emissive_voxel_lighting,
            vulkan_ctx: &self.vulkan_ctx,
        };
        self.shutdown_lifecycle.drain(&mut actions)
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
    use super::{AppShutdownLifecycle, ShutdownActions, ShutdownState};
    use anyhow::{anyhow, Result};

    #[derive(Default)]
    struct FakeShutdownActions {
        calls: Vec<&'static str>,
        water_failures_remaining: usize,
        discard_failures_remaining: usize,
    }

    impl ShutdownActions for FakeShutdownActions {
        fn shutdown_water(&mut self) -> Result<()> {
            self.calls.push("water");
            if self.water_failures_remaining > 0 {
                self.water_failures_remaining -= 1;
                return Err(anyhow!("injected water shutdown failure"));
            }
            Ok(())
        }

        fn discard_contree_readback(&mut self) -> Result<()> {
            self.calls.push("discard");
            if self.discard_failures_remaining > 0 {
                self.discard_failures_remaining -= 1;
                return Err(anyhow!("injected Contree discard failure"));
            }
            Ok(())
        }

        fn join_dependent_workers(&mut self) {
            self.calls.push("join");
        }

        fn wait_device_idle(&mut self) {
            self.calls.push("idle");
        }
    }

    #[test]
    fn failed_shutdown_attempts_every_phase_and_remains_retryable() {
        let mut lifecycle = AppShutdownLifecycle::default();
        assert!(lifecycle.begin());
        let mut actions = FakeShutdownActions {
            water_failures_remaining: 1,
            discard_failures_remaining: 1,
            ..FakeShutdownActions::default()
        };

        let error = lifecycle.drain(&mut actions).unwrap_err();

        assert_eq!(actions.calls, ["water", "discard", "join", "idle"]);
        assert!(error.to_string().contains("water"));
        assert!(error.to_string().contains("Contree"));
        assert_eq!(lifecycle.state(), ShutdownState::Quiescing);

        actions.calls.clear();
        lifecycle.drain(&mut actions).unwrap();
        assert_eq!(actions.calls, ["water", "discard", "idle"]);
        assert_eq!(lifecycle.state(), ShutdownState::Complete);
    }

    #[test]
    fn successful_phases_are_not_repeated_but_idle_covers_each_retry() {
        let mut lifecycle = AppShutdownLifecycle::default();
        lifecycle.begin();
        let mut actions = FakeShutdownActions {
            water_failures_remaining: 1,
            ..FakeShutdownActions::default()
        };

        assert!(lifecycle.drain(&mut actions).is_err());
        assert_eq!(actions.calls, ["water", "discard", "join", "idle"]);

        actions.calls.clear();
        lifecycle.drain(&mut actions).unwrap();
        assert_eq!(actions.calls, ["water", "idle"]);

        actions.calls.clear();
        assert!(!lifecycle.begin());
        lifecycle.drain(&mut actions).unwrap();
        assert!(actions.calls.is_empty());
    }
}
