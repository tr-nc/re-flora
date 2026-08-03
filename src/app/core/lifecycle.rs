use super::App;
use winit::event_loop::ActiveEventLoop;

impl App {
    pub fn on_terminate(&mut self, event_loop: &ActiveEventLoop) {
        self.stop_terrain_edit_loop_sound();
        self.vulkan_ctx.device().wait_idle();
        event_loop.exit();
    }

    pub fn on_about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(test) = self.resize_lifecycle_test.as_mut() {
            test.request_next(
                &self.window_state.window(),
                self.time_info.total_frame_count(),
            );
        }
        if !self.window_state.is_minimized() {
            self.window_state.window().request_redraw();
        }
    }

    pub(super) fn on_resize(&mut self) {
        self.frame_manager.wait_for_all_submissions();
        // A frame-submit fence covers command execution, but the presentation operation can still
        // be consuming the render-finished semaphore when the fence is observed.  Wait only the
        // presentation queue before destroying swapchain-owned semaphores/images; this preserves
        // the no-device-wide-idle resize contract while satisfying Vulkan lifetime rules.
        self.vulkan_ctx
            .device()
            .wait_queue_idle(&self.vulkan_ctx.get_general_queue());

        let window_extent = self.window_state.window_extent();

        self.swapchain.on_resize(window_extent);
        self.frame_manager
            .recreate_swapchain_images(self.vulkan_ctx.device(), self.swapchain.image_count());
        self.tracer.on_resize(
            window_extent,
            self.contree_builder.get_resources(),
            self.scene_accel_builder.get_resources(),
            self.plain_builder.get_resources(),
        );

        self.egui_renderer
            .set_render_pass(self.swapchain.get_render_pass());

        self.resize_generation = self
            .resize_generation
            .checked_add(1)
            .expect("resize generation overflow");
        if let Some(test) = self.resize_lifecycle_test.as_mut() {
            test.observe(window_extent, self.tracer.extent_resource_generation());
        }
        log::info!(
            "[RESIZE] published generation={} extent={}x{} tracer_extent_generation={}",
            self.resize_generation,
            window_extent.width,
            window_extent.height,
            self.tracer.extent_resource_generation(),
        );

        self.is_resize_pending = false;
    }
}
