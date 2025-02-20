use crate::util::time_info::TimeInfo;
use crate::{
    renderer::Renderer,
    vkn::{
        context::{ContextCreateInfo, VulkanContext},
        swapchain::Swapchain,
    },
    window::{WindowDescriptor, WindowMode, WindowState},
};
use ash::{vk, Device};
use egui::{ClippedPrimitive, Color32, RichText, Slider, TextureId, ViewportId};
use gpu_allocator::vulkan::{Allocator, AllocatorCreateDesc};
use std::sync::{Arc, Mutex};
use winit::event::DeviceEvent;
use winit::{
    event::{ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
    window::WindowId,
};

pub struct InitializedApp {
    vulkan_context: VulkanContext,
    egui_context: egui::Context,
    egui_winit_state: egui_winit::State,
    renderer: Renderer,
    window_state: WindowState,
    is_resize_pending: bool,
    textures_to_free: Option<Vec<TextureId>>,
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
        let vulkan_context = Self::create_vulkan_context(&window_state);

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

        let egui_context = egui::Context::default();
        let egui_winit_state = egui_winit::State::new(
            egui_context.clone(),
            ViewportId::ROOT,
            &window_state.window(),
            None,
            None,
            None,
        );

        let cmdbuf = Self::create_cmdbuf(&vulkan_context.device, vulkan_context.command_pool);

        let renderer = Self::create_renderer(&vulkan_context, &swapchain);

        Self {
            vulkan_context,
            egui_context,
            egui_winit_state,
            renderer,
            window_state,
            cmdbuf,
            swapchain,
            image_available_semaphore,
            render_finished_semaphore,
            fence,

            is_resize_pending: false,
            time_info: TimeInfo::default(),
            textures_to_free: None,
            slider_val: 0.0,
        }
    }

    fn create_window_state(event_loop: &ActiveEventLoop) -> WindowState {
        let window_descriptor = WindowDescriptor {
            title: "Flora".to_owned(),
            window_mode: WindowMode::BorderlessFullscreen,
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

    fn create_renderer(vulkan_context: &VulkanContext, swapchain: &Swapchain) -> Renderer {
        let allocator = Allocator::new(&AllocatorCreateDesc {
            instance: vulkan_context.instance.clone(),
            device: vulkan_context.device.clone(),
            physical_device: vulkan_context.physical_device,
            debug_settings: Default::default(),
            buffer_device_address: false,
            allocation_sizes: Default::default(),
        })
        .expect("Failed to create allocator");

        Renderer::with_gpu_allocator(
            Arc::new(Mutex::new(allocator)),
            vulkan_context.device.clone(),
            swapchain.render_pass,
            crate::renderer::RendererOptions {
                srgb_framebuffer: true,
                ..Default::default()
            },
        )
    }

    pub fn on_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        // if cursor is visible, feed the event to gui first, if the event is being consumed by gui, no need to handle it again later
        if self.window_state.is_cursor_visible() {
            // let consumed = self.gui.update(&event);

            let consumed = self
                .egui_winit_state
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
                event_loop.exit();

                log::info!("Stopping application");

                unsafe {
                    self.vulkan_context
                        .device
                        .device_wait_idle()
                        .expect("Failed to wait for graphics device to idle.");

                    // self.egui_app.clean(&self.context);

                    self.vulkan_context.device.destroy_fence(self.fence, None);
                    self.vulkan_context
                        .device
                        .destroy_semaphore(self.image_available_semaphore, None);
                    self.vulkan_context
                        .device
                        .destroy_semaphore(self.render_finished_semaphore, None);

                    self.swapchain.destroy(&self.vulkan_context);

                    self.vulkan_context
                        .device
                        .free_command_buffers(self.vulkan_context.command_pool, &[self.cmdbuf]);
                }
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
                // close the loop when escape key is pressed
                if event.state == ElementState::Pressed && event.physical_key == KeyCode::Escape {
                    event_loop.exit();
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
                    self.resize();
                }

                //
                self.time_info.update();
                // self.camera.update_transform(self.time_info.delta_time());

                unsafe {
                    self.vulkan_context
                        .device
                        .wait_for_fences(&[self.fence], true, std::u64::MAX)
                        .expect("Failed to wait ")
                };

                // Free last frames textures after the previous frame is done rendering
                if let Some(textures) = self.textures_to_free.take() {
                    self.renderer.free_textures(&textures);
                }

                // Generate UI
                let raw_input = self
                    .egui_winit_state
                    .take_egui_input(&self.window_state.window());

                let egui::FullOutput {
                    platform_output,
                    textures_delta,
                    shapes,
                    pixels_per_point,
                    ..
                } = self.egui_context.run(raw_input, |ctx| {
                    // self.egui_app.build_ui(ctx); TODO:

                    let my_frame = egui::containers::Frame {
                        fill: Color32::from_rgba_premultiplied(0, 0, 0, 128),
                        ..Default::default()
                    };

                    // https://github.com/emilk/egui/blob/master/crates/egui_demo_lib/src/demo/panels.rs
                    egui::SidePanel::left("left_panel")
                        .frame(my_frame)
                        .resizable(true)
                        .default_width(300.0)
                        .show(&ctx, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.heading("Left Panel");
                            });
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                ui.label(RichText::new(format!(
                                    "fps: {:.2}",
                                    self.time_info.display_fps()
                                )));
                                // https://github.com/emilk/egui/blob/master/crates/egui_demo_lib/src/demo/sliders.rs
                                ui.add(Slider::new(&mut self.slider_val, 0.0..=1.0).text("Slider"));
                            });
                        });
                });

                self.egui_winit_state
                    .handle_platform_output(&self.window_state.window(), platform_output);

                if !textures_delta.free.is_empty() {
                    self.textures_to_free = Some(textures_delta.free.clone());
                }

                if !textures_delta.set.is_empty() {
                    self.renderer.set_textures(
                        self.vulkan_context.get_general_queue(),
                        self.vulkan_context.command_pool,
                        textures_delta.set.as_slice(),
                    );
                }

                let clipped_primitives = self.egui_context.tessellate(shapes, pixels_per_point);

                // Drawing the frame
                let next_image_result = unsafe {
                    self.swapchain.swapchain_device.acquire_next_image(
                        self.swapchain.swapchain_khr,
                        std::u64::MAX,
                        self.image_available_semaphore,
                        vk::Fence::null(),
                    )
                };
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

                // Re-record commands to draw geometry
                record_command_buffers(
                    &self.vulkan_context.device,
                    self.vulkan_context.command_pool,
                    self.cmdbuf,
                    self.swapchain.framebuffers[image_index as usize],
                    self.swapchain.render_pass,
                    vk::Extent2D {
                        width: self.window_state.window_size()[0],
                        height: self.window_state.window_size()[1],
                    },
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

                let swapchains = [self.swapchain.swapchain_khr];
                let images_indices = [image_index];
                let present_info = vk::PresentInfoKHR::default()
                    .wait_semaphores(&signal_semaphores)
                    .swapchains(&swapchains)
                    .image_indices(&images_indices);

                let present_result = unsafe {
                    self.swapchain
                        .swapchain_device
                        .queue_present(self.vulkan_context.get_general_queue(), &present_info)
                };
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

    fn resize(&mut self) {
        // Resize the window here

        let window_size = self.window_state.window_size();

        self.swapchain.recreate(&self.vulkan_context, &window_size);

        self.renderer.set_render_pass(self.swapchain.render_pass);

        self.is_resize_pending = false;
    }
}

fn record_command_buffers(
    device: &Device,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    framebuffer: vk::Framebuffer,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    pixels_per_point: f32,
    renderer: &mut Renderer,

    clipped_primitives: &[ClippedPrimitive],
) {
    unsafe {
        device
            .reset_command_pool(command_pool, vk::CommandPoolResetFlags::empty())
            .expect("Failed to reset command pool")
    };

    let command_buffer_begin_info =
        vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::SIMULTANEOUS_USE);
    unsafe {
        device
            .begin_command_buffer(command_buffer, &command_buffer_begin_info)
            .expect("Failed to begin command buffer")
    };

    let render_pass_begin_info = vk::RenderPassBeginInfo::default()
        .render_pass(render_pass)
        .framebuffer(framebuffer)
        .render_area(vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent,
        })
        .clear_values(&[vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.007, 0.007, 0.007, 1.0],
            },
        }]);

    unsafe {
        device.cmd_begin_render_pass(
            command_buffer,
            &render_pass_begin_info,
            vk::SubpassContents::INLINE,
        )
    };

    renderer.cmd_draw(command_buffer, extent, pixels_per_point, clipped_primitives);

    unsafe { device.cmd_end_render_pass(command_buffer) };

    unsafe { device.end_command_buffer(command_buffer).unwrap() };
}
