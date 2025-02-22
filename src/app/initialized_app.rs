use crate::util::time_info::TimeInfo;
use crate::vkn::ShaderCompiler;
use crate::{
    renderer::Renderer,
    renderer::RendererOptions,
    vkn::{
        context::{ContextCreateInfo, VulkanContext},
        swapchain::Swapchain,
    },
    window::{WindowMode, WindowState, WindowStateDesc},
};
use ash::vk::Extent2D;
use ash::{vk, Device};
use egui::{ClippedPrimitive, Color32, RichText, Slider};
use std::sync::Arc;
use winit::event::DeviceEvent;
use winit::{
    event::{ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
    window::WindowId,
};

pub struct InitializedApp {
    vulkan_context: Arc<VulkanContext>,
    _shader_compiler: ShaderCompiler,

    renderer: Renderer,

    window_state: WindowState,
    is_resize_pending: bool,
    cmdbuf: vk::CommandBuffer,
    swapchain: Swapchain,
    image_available_semaphore: vk::Semaphore,
    render_finished_semaphore: vk::Semaphore,
    fence: vk::Fence,
    time_info: TimeInfo,

    slider_val: f32,
}

impl InitializedApp {
    pub fn new(_event_loop: &ActiveEventLoop) -> Self {
        let window_state = Self::create_window_state(_event_loop);
        let vulkan_context = Arc::new(Self::create_vulkan_context(&window_state));

        let shader_compiler = ShaderCompiler::new(Default::default()).unwrap();

        let swapchain = Swapchain::new(
            &vulkan_context,
            &window_state.window_size(),
            Default::default(),
        );

        let (image_available_semaphore, render_finished_semaphore) =
            Self::create_semaphores(&vulkan_context.device);

        let fence = Self::create_fence(&vulkan_context.device);

        // enable for image loading feature of egui
        // egui_extras::install_image_loaders(&context);

        let cmdbuf = Self::create_cmdbuf(&vulkan_context.device, vulkan_context.command_pool);

        let renderer = Renderer::new(
            &vulkan_context,
            &window_state.window(),
            &shader_compiler,
            swapchain.get_render_pass(),
            RendererOptions {
                srgb_framebuffer: true,
                ..Default::default()
            },
        );

        Self {
            vulkan_context,
            _shader_compiler: shader_compiler,
            renderer,
            window_state,
            cmdbuf,
            swapchain,
            image_available_semaphore,
            render_finished_semaphore,
            fence,

            is_resize_pending: false,
            time_info: TimeInfo::default(),
            slider_val: 0.0,
        }
    }

    fn create_window_state(event_loop: &ActiveEventLoop) -> WindowState {
        let window_descriptor = WindowStateDesc {
            title: "Flora".to_owned(),
            window_mode: WindowMode::Windowed,
            cursor_locked: true,
            cursor_visible: false,
            ..Default::default()
        };
        WindowState::new(event_loop, &window_descriptor)
    }

    fn create_vulkan_context(window_state: &WindowState) -> VulkanContext {
        VulkanContext::new(
            &window_state.window(),
            ContextCreateInfo {
                name: "Flora".into(),
            },
        )
    }

    /// Returns: (image_available_semaphore, render_finished_semaphore)
    fn create_semaphores(device: &Device) -> (vk::Semaphore, vk::Semaphore) {
        let image_available_semaphore = {
            let semaphore_info = vk::SemaphoreCreateInfo::default();
            unsafe { device.create_semaphore(&semaphore_info, None).unwrap() }
        };

        let render_finished_semaphore = {
            let semaphore_info = vk::SemaphoreCreateInfo::default();
            unsafe { device.create_semaphore(&semaphore_info, None).unwrap() }
        };

        (image_available_semaphore, render_finished_semaphore)
    }

    fn create_fence(device: &Device) -> vk::Fence {
        let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
        unsafe { device.create_fence(&fence_info, None).unwrap() }
    }

    fn create_cmdbuf(device: &Device, command_pool: vk::CommandPool) -> vk::CommandBuffer {
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        unsafe { device.allocate_command_buffers(&allocate_info).unwrap()[0] }
    }

    pub fn on_terminate(&mut self, event_loop: &ActiveEventLoop) {
        // ensure all command buffers are done executing before terminating anything
        self.vulkan_context.wait_device_idle().unwrap();

        event_loop.exit();
        unsafe {
            self.vulkan_context.device.destroy_fence(self.fence, None);
            self.vulkan_context
                .device
                .destroy_semaphore(self.image_available_semaphore, None);
            self.vulkan_context
                .device
                .destroy_semaphore(self.render_finished_semaphore, None);
        }
    }

    pub fn on_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        // if cursor is visible, feed the event to gui first, if the event is being consumed by gui, no need to handle it again later
        if self.window_state.is_cursor_visible() {
            let consumed = self
                .renderer
                .on_window_event(&self.window_state.window(), &event)
                .consumed;

            if consumed {
                println!("Event consumed by egui");
                return;
            }
        }

        match event {
            // close the loop, therefore the window, when close button is clicked
            WindowEvent::CloseRequested => {
                self.on_terminate(event_loop);
            }

            // never happened and never tested, take caution
            WindowEvent::ScaleFactorChanged {
                scale_factor: _scale_factor,
                inner_size_writer: _inner_size_writer,
            } => {
                self.is_resize_pending = true;
            }

            // resize the window
            WindowEvent::Resized(_) => {
                self.is_resize_pending = true;
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed && event.physical_key == KeyCode::Escape {
                    self.on_terminate(event_loop);
                    return;
                }

                if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyE {
                    self.window_state.toggle_cursor_visibility();
                    self.window_state.toggle_cursor_grab();
                }

                if !self.window_state.is_cursor_visible() {
                    // self.camera.handle_keyboard(&event);
                }
            }

            // redraw the window
            WindowEvent::RedrawRequested => {
                // when the windiw is resized, redraw is called afterwards, so when the window is minimized, return
                if self.window_state.is_minimized() {
                    return;
                }

                // resize the window if needed
                if self.is_resize_pending {
                    self.on_resize();
                }

                self.time_info.update();
                // self.camera.update_transform(self.time_info.delta_time());

                self.vulkan_context.wait_for_fences(&[self.fence]).unwrap();

                let (pixels_per_point, clipped_primitives) =
                    self.renderer.update(&self.window_state.window(), |ctx| {
                        let my_frame = egui::containers::Frame {
                            fill: Color32::from_rgba_premultiplied(50, 0, 10, 128),
                            ..Default::default()
                        };

                        egui::SidePanel::left("left_panel")
                            .frame(my_frame)
                            .resizable(true)
                            .default_width(300.0)
                            .show(&ctx, |ui| {
                                ui.vertical_centered(|ui| {
                                    ui.heading("Re: Flora");
                                });
                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    ui.label(RichText::new(format!(
                                        "fps: {:.2}",
                                        self.time_info.display_fps()
                                    )));
                                    ui.add(
                                        Slider::new(&mut self.slider_val, 0.0..=1.0).text("Slider"),
                                    );
                                });
                            });
                    });

                let next_image_result = self
                    .swapchain
                    .acquire_next_image(&self.image_available_semaphore);

                let image_index = match next_image_result {
                    Ok((image_index, _)) => image_index,
                    Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                        self.is_resize_pending = true;
                        return;
                    }
                    Err(error) => panic!("Error while acquiring next image. Cause: {}", error),
                };

                unsafe {
                    self.vulkan_context
                        .device
                        .reset_fences(&[self.fence])
                        .expect("Failed to reset fences")
                };

                let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
                let wait_semaphores = [self.image_available_semaphore];
                let signal_semaphores = [self.render_finished_semaphore];

                let render_area = Extent2D {
                    width: self.window_state.window_size()[0],
                    height: self.window_state.window_size()[1],
                };

                record_command_buffers(
                    &self.vulkan_context.device,
                    self.vulkan_context.command_pool,
                    self.cmdbuf,
                    &self.swapchain,
                    image_index,
                    render_area,
                    pixels_per_point,
                    &mut self.renderer,
                    &clipped_primitives,
                );

                let command_buffers = [self.cmdbuf];
                let submit_info = [vk::SubmitInfo::default()
                    .wait_semaphores(&wait_semaphores)
                    .wait_dst_stage_mask(&wait_stages)
                    .command_buffers(&command_buffers)
                    .signal_semaphores(&signal_semaphores)];
                unsafe {
                    self.vulkan_context
                        .device
                        .queue_submit(
                            self.vulkan_context.get_general_queue(),
                            &submit_info,
                            self.fence,
                        )
                        .expect("Failed to submit work to gpu.")
                };

                let present_result = self.swapchain.present(&signal_semaphores, image_index);

                match present_result {
                    Ok(is_suboptimal) if is_suboptimal => {
                        self.is_resize_pending = true;
                    }
                    Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                        self.is_resize_pending = true;
                    }
                    Err(error) => panic!("Failed to present queue. Cause: {}", error),
                    _ => {}
                }
            }
            _ => (),
        }
    }

    pub fn on_device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        match event {
            DeviceEvent::MouseMotion { delta: _delta } => {
                if !self.window_state.is_cursor_visible() {
                    // self.camera.handle_mouse(&delta);
                }
            }
            _ => (),
        }

        // Handle device events here
    }

    pub fn on_about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if !self.window_state.is_minimized() {
            self.window_state.window().request_redraw();
        }
    }

    fn on_resize(&mut self) {
        // Resize the window here

        let window_size = self.window_state.window_size();

        self.swapchain.on_resize(&self.vulkan_context, &window_size);

        // the render pass is rebuilt when the swapchain is recreated
        self.renderer
            .set_render_pass(self.swapchain.get_render_pass());

        self.is_resize_pending = false;
    }
}

// fn record_command_buffers(
//     device: &Device,
//     command_pool: vk::CommandPool,
//     command_buffer: vk::CommandBuffer,
//     swapchain: &Swapchain,
//     image_index: u32,
//     render_area: Extent2D,
//     pixels_per_point: f32,
//     renderer: &mut Renderer,

//     clipped_primitives: &[ClippedPrimitive],
// ) {
//     unsafe {
//         device
//             .reset_command_pool(command_pool, vk::CommandPoolResetFlags::empty())
//             .expect("Failed to reset command pool")
//     };

//     let command_buffer_begin_info =
//         vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::SIMULTANEOUS_USE);

//     unsafe {
//         device
//             .begin_command_buffer(command_buffer, &command_buffer_begin_info)
//             .expect("Failed to begin command buffer")
//     };

//     swapchain.record_begin_render_pass_cmdbuf(command_buffer, image_index, &render_area);

//     renderer.cmd_draw(
//         command_buffer,
//         render_area,
//         pixels_per_point,
//         clipped_primitives,
//     );

//     unsafe { device.cmd_end_render_pass(command_buffer) };
//     unsafe { device.end_command_buffer(command_buffer).unwrap() };
// }
