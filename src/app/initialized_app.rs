use crate::builder::{AccelStructBuilder, OctreeBuilder, PlainBuilder};
use crate::gameplay::{Camera, CameraDesc};
use crate::tracer::Tracer;
use crate::util::TimeInfo;
use crate::util::{ShaderCompiler, Timer};
use crate::vkn::{Allocator, CommandBuffer, Fence, Semaphore, SwapchainDesc};
use crate::{
    egui_renderer::EguiRenderer,
    vkn::{Swapchain, VulkanContext, VulkanContextDesc},
    window::{WindowMode, WindowState, WindowStateDesc},
};
use ash::vk;
use egui::{Color32, RichText};
use glam::{UVec3, Vec2, Vec3};
use gpu_allocator::vulkan::AllocatorCreateDesc;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use winit::event::DeviceEvent;
use winit::{
    event::{ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
    window::WindowId,
};

pub struct InitializedApp {
    egui_renderer: EguiRenderer,
    cmdbuf: CommandBuffer,
    window_state: WindowState,
    is_resize_pending: bool,
    swapchain: Swapchain,
    image_available_semaphore: Semaphore,
    render_finished_semaphore: Semaphore,
    fence: Fence,
    time_info: TimeInfo,
    accumulated_mouse_delta: Vec2,
    smoothed_mouse_delta: Vec2,

    camera: Camera,
    tracer: Tracer,

    // builders
    plain_builder: PlainBuilder,
    accel_struct_builder: AccelStructBuilder,
    octree_builder: OctreeBuilder,

    // gui adjustables
    debug_float: f32,

    // note: always keep the context to end, as it has to be destroyed last
    vulkan_ctx: VulkanContext,
}

impl InitializedApp {
    pub fn new(_event_loop: &ActiveEventLoop) -> Self {
        let window_state = Self::create_window_state(_event_loop);
        let vulkan_ctx = Self::create_vulkan_context(&window_state);

        let shader_compiler = ShaderCompiler::new().unwrap();

        let device = vulkan_ctx.device();

        let gpu_allocator = {
            let allocator_create_info = AllocatorCreateDesc {
                instance: vulkan_ctx.instance().as_raw().clone(),
                device: device.as_raw().clone(),
                physical_device: vulkan_ctx.physical_device().as_raw(),
                debug_settings: Default::default(),
                buffer_device_address: true,
                allocation_sizes: Default::default(),
            };
            gpu_allocator::vulkan::Allocator::new(&allocator_create_info)
                .expect("Failed to create gpu allocator")
        };
        let allocator = Allocator::new(device, Arc::new(Mutex::new(gpu_allocator)));

        let swapchain = Swapchain::new(
            vulkan_ctx.clone(),
            &window_state.window_size(),
            SwapchainDesc {
                present_mode: vk::PresentModeKHR::MAILBOX,
                ..Default::default()
            },
        );

        let image_available_semaphore = Semaphore::new(device);
        let render_finished_semaphore = Semaphore::new(device);

        let fence = Fence::new(device, true);

        let cmdbuf = CommandBuffer::new(device, vulkan_ctx.command_pool());

        let renderer = EguiRenderer::new(
            &vulkan_ctx,
            &window_state.window(),
            &allocator,
            &shader_compiler,
            swapchain.get_render_pass(),
        );

        let screen_extent = window_state.window_size();

        let camera = Camera::new(
            Vec3::new(-0.5, 0.6, -0.5),
            135.0,
            -5.0,
            CameraDesc {
                movement: Default::default(),
                projection: Default::default(),
                aspect_ratio: screen_extent[0] as f32 / screen_extent[1] as f32,
            },
        );

        let plain_builder = PlainBuilder::new(
            vulkan_ctx.clone(),
            &shader_compiler,
            allocator.clone(),
            UVec3::new(256, 256, 256) * UVec3::new(5, 5, 5), // plain atlas dim
            UVec3::new(512, 512, 512),                       // free atlas dim
        );

        let mut accel_struct_builder = AccelStructBuilder::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &shader_compiler,
            UVec3::new(4, 4, 4),
            20 * 20 * 20, // tlas instance cap
        );

        // 64^3 chunk, a total scene size of 1024 * 512 * 1024
        let mut test_map = HashMap::new();
        for i in 0..1 {
            for j in 0..1 {
                for k in 0..1 {
                    test_map.insert(UVec3::new(i, j, k), i * 100 + j * 10 + k);
                }
            }
        }
        let timer = Timer::new();
        accel_struct_builder.build_chunks_tlas(test_map);
        log::debug!("Tlas build time: {:?}", timer.elapsed());

        let octree_builder = OctreeBuilder::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &shader_compiler,
            plain_builder.resources(),
            UVec3::new(256, 256, 256), // max voxel dim per chunk
            10_000_000,                // octree buffer pool size
        );

        let tracer = Tracer::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &shader_compiler,
            &screen_extent,
            octree_builder.get_resources(),
        );

        let mut this = Self {
            vulkan_ctx,
            egui_renderer: renderer,
            window_state,

            accumulated_mouse_delta: glam::Vec2::ZERO,
            smoothed_mouse_delta: glam::Vec2::ZERO,

            cmdbuf,
            swapchain,
            image_available_semaphore,
            render_finished_semaphore,
            fence,

            tracer,

            plain_builder,
            accel_struct_builder,
            octree_builder,

            camera,
            is_resize_pending: false,
            time_info: TimeInfo::default(),

            debug_float: 0.0,
        };
        this.init();
        return this;
    }

    fn init(&mut self) {
        // Chunk avg init time: 28.55ms for 512^3 chunk
        // Chunk avg init time: 3.8ms for 256^3 chunk
        // Chunk avg init time: 0.65ms for 128^3 chunk
        // Chunk avg init time: 0.183ms for 64^3 chunk
        // use bigger chunk size, for smaller overhead
        // use workgroup size of 4^3 works better than 8^3
        self.plain_builder
            .chunk_init(UVec3::new(0, 0, 0), UVec3::new(256, 256, 256));

        // Oct avg init time: 6.55ms for 512^3 chunk
        // Oct avg init time: 1.33ms for 256^3 chunk
        // Oct avg init time: 0.55ms for 128^3 chunk
        // Oct avg init time: 0.41ms for 64^3 chunk
        self.octree_builder
            .build_and_alloc(UVec3::new(0, 0, 0), UVec3::new(256, 256, 256))
            .unwrap();
        // log::debug!("Oct avg init time: {:?}", timer.elapsed() / test_times);
    }

    fn create_window_state(event_loop: &ActiveEventLoop) -> WindowState {
        let window_descriptor = WindowStateDesc {
            title: "Re: Flora".to_owned(),
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
            VulkanContextDesc {
                name: "Re: Flora".into(),
            },
        )
    }

    pub fn on_terminate(&mut self, event_loop: &ActiveEventLoop) {
        // ensure all command buffers are done executing before terminating anything
        self.vulkan_ctx.device().wait_idle();
        event_loop.exit();
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
                .egui_renderer
                .on_window_event(&self.window_state.window(), &event)
                .consumed;

            if consumed {
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
                    self.camera.handle_keyboard(&event);
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
                let frame_delta_time = self.time_info.delta_time();

                if !self.window_state.is_cursor_visible() {
                    // grab the value and immediately reset the accumulator
                    let mouse_delta = self.accumulated_mouse_delta;
                    self.accumulated_mouse_delta = glam::Vec2::ZERO;

                    let alpha = 0.4; // mouse smoothing factor: 0 = no smoothing, 1 = infinite smoothing
                    self.smoothed_mouse_delta =
                        self.smoothed_mouse_delta * alpha + mouse_delta * (1.0 - alpha);

                    self.camera.handle_mouse(self.smoothed_mouse_delta);
                }

                self.camera.update_transform(frame_delta_time);

                self.vulkan_ctx
                    .wait_for_fences(&[self.fence.as_raw()])
                    .unwrap();

                self.egui_renderer
                    .update(&self.window_state.window(), |ctx| {
                        let my_frame = egui::containers::Frame {
                            fill: Color32::from_rgba_premultiplied(50, 0, 10, 128),
                            inner_margin: egui::Margin::same(10),
                            ..Default::default()
                        };

                        egui::SidePanel::left("left_panel")
                            .frame(my_frame)
                            .resizable(true)
                            .default_width(300.0)
                            .show(&ctx, |ui| {
                                ui.vertical_centered(|ui| {
                                    ui.heading("Config Panel");
                                });
                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    ui.label(RichText::new(format!(
                                        "fps: {:.2}",
                                        self.time_info.display_fps()
                                    )));

                                    ui.add(
                                        egui::Slider::new(&mut self.debug_float, 0.0..=1.0)
                                            .text("Debug Float"),
                                    );
                                });
                            });
                    });

                let device = self.vulkan_ctx.device();

                let image_idx = match self.swapchain.acquire_next(&self.image_available_semaphore) {
                    Ok((image_index, _)) => image_index,
                    Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                        self.is_resize_pending = true;
                        return;
                    }
                    Err(error) => panic!("Error while acquiring next image. Cause: {}", error),
                };

                unsafe {
                    device
                        .as_raw()
                        .reset_fences(&[self.fence.as_raw()])
                        .expect("Failed to reset fences")
                };

                self.tracer
                    .update_buffers(self.debug_float, &self.camera, 0)
                    .unwrap();

                let cmdbuf = &self.cmdbuf;
                cmdbuf.begin(false);

                self.tracer
                    .record_command_buffer(cmdbuf, &self.window_state.window_size());

                self.swapchain
                    .record_blit(self.tracer.get_dst_image(), cmdbuf, image_idx);

                let render_area = vk::Extent2D {
                    width: self.window_state.window_size()[0],
                    height: self.window_state.window_size()[1],
                };

                self.swapchain
                    .record_begin_render_pass_cmdbuf(cmdbuf, image_idx, &render_area);

                self.egui_renderer
                    .record_command_buffer(device, cmdbuf, render_area);

                unsafe {
                    device.cmd_end_render_pass(cmdbuf.as_raw());
                };

                cmdbuf.end();

                let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
                let wait_semaphores = [self.image_available_semaphore.as_raw()];
                let signal_semaphores = [self.render_finished_semaphore.as_raw()];
                let command_buffers = [self.cmdbuf.as_raw()];
                let submit_info = [vk::SubmitInfo::default()
                    .wait_semaphores(&wait_semaphores)
                    .wait_dst_stage_mask(&wait_stages)
                    .command_buffers(&command_buffers)
                    .signal_semaphores(&signal_semaphores)];

                unsafe {
                    self.vulkan_ctx
                        .device()
                        .as_raw()
                        .queue_submit(
                            self.vulkan_ctx.get_general_queue().as_raw(),
                            &submit_info,
                            self.fence.as_raw(),
                        )
                        .expect("Failed to submit work to gpu.")
                };

                let present_result = self.swapchain.present(&signal_semaphores, image_idx);

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
        if let DeviceEvent::MouseMotion { delta } = event {
            if !self.window_state.is_cursor_visible() {
                self.accumulated_mouse_delta += Vec2::new(delta.0 as f32, delta.1 as f32);
            }
        }
    }

    pub fn on_about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if !self.window_state.is_minimized() {
            self.window_state.window().request_redraw();
        }
    }

    fn on_resize(&mut self) {
        self.vulkan_ctx.device().wait_idle();

        let window_size = self.window_state.window_size();

        self.camera.on_resize(&window_size);
        self.tracer
            .on_resize(&window_size, self.octree_builder.get_resources());
        self.swapchain.on_resize(&window_size);

        // the render pass should be rebuilt when the swapchain is recreated
        self.egui_renderer
            .set_render_pass(self.swapchain.get_render_pass());

        self.is_resize_pending = false;
    }
}
