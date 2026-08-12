use super::App;
use anyhow::{Context, Result};
use winit::event_loop::ActiveEventLoop;

impl App {
    pub fn on_terminate(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(err) = self.shutdown_for_termination() {
            panic!("[SHUTDOWN] failed to drain application GPU work: {err:#}");
        }
        event_loop.exit();
    }

    pub fn on_about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if self.shutdown_started {
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

    /// Runs the one legal application teardown transition. The flag is set
    /// before any blocking operation so a re-entrant event cannot submit more
    /// work while shutdown consumes the already-submitted jobs.
    pub(super) fn shutdown_for_termination(&mut self) -> Result<()> {
        if self.shutdown_started {
            return Ok(());
        }
        self.shutdown_started = true;

        log::info!("[SHUTDOWN] phase=quiesce_producers");
        self.stop_terrain_edit_loop_sound();
        self.water_sim.shutdown();

        log::info!("[SHUTDOWN] phase=consume_managed_gpu_jobs");
        self.abort_loading_physical_publication();
        self.shutdown_water_terrain()
            .context("shut down water terrain runtime")?;
        self.contree_builder
            .discard_active_cpu_chunk_cache_job()
            .context("discard active Contree CPU-cache GPU readback")?;

        log::info!("[SHUTDOWN] phase=join_dependent_workers");
        self.contree_builder.shutdown_cpu_chunk_cache_worker();

        log::info!("[SHUTDOWN] phase=wait_device_idle");
        self.vulkan_ctx.device().wait_idle();
        log::info!("[SHUTDOWN] phase=complete");
        Ok(())
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
