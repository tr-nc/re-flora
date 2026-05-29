use super::App;
use winit::event_loop::ActiveEventLoop;

impl App {
    pub fn on_terminate(&mut self, event_loop: &ActiveEventLoop) {
        self.stop_terrain_edit_loop_sound();
        self.vulkan_ctx.device().wait_idle();
        event_loop.exit();
    }

    pub fn on_about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if !self.window_state.is_minimized() {
            self.window_state.window().request_redraw();
        }
    }

    pub(super) fn on_resize(&mut self) {
        self.vulkan_ctx.device().wait_idle();

        let window_extent = self.window_state.window_extent();

        self.swapchain.on_resize(window_extent);
        self.frame_manager
            .recreate_swapchain_images(self.vulkan_ctx.device(), self.swapchain.image_count());
        self.tracer.on_resize(
            window_extent,
            self.contree_builder.get_resources(),
            self.scene_accel_builder.get_resources(),
        );

        self.egui_renderer
            .set_render_pass(self.swapchain.get_render_pass());

        self.is_resize_pending = false;
    }
}
