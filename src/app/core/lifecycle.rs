use super::App;
use re_flora_vkn::{Device, Fence, Semaphore};
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
        self.rebuild_swapchain_image_syncs();
        self.tracer.on_resize(
            window_extent,
            self.contree_builder.get_resources(),
            self.scene_accel_builder.get_resources(),
        );

        self.egui_renderer
            .set_render_pass(self.swapchain.get_render_pass());

        self.is_resize_pending = false;
    }

    fn rebuild_swapchain_image_syncs(&mut self) {
        let device = self.vulkan_ctx.device();
        let image_count = self.swapchain.image_count();
        let (present_semaphores, images_in_flight) =
            Self::create_swapchain_image_syncs(device, image_count);
        self.image_render_finished_semaphores = present_semaphores;
        self.images_in_flight = images_in_flight;
    }

    pub(super) fn create_swapchain_image_syncs(
        device: &Device,
        image_count: usize,
    ) -> (Vec<Semaphore>, Vec<Option<Fence>>) {
        let semaphores = (0..image_count).map(|_| Semaphore::new(device)).collect();
        let images_in_flight = vec![None; image_count];
        (semaphores, images_in_flight)
    }
}
