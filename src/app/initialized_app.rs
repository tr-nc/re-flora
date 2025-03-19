use crate::gameplay::{Camera, CameraDesc};
use crate::util::compiler::ShaderCompiler;
use crate::util::time_info::TimeInfo;
use crate::vkn::{
    Allocator, Buffer, BufferBuilder, CommandBuffer, CommandPool, ComputePipeline, DescriptorPool,
    DescriptorSet, Device, Fence, Semaphore, ShaderModule, Texture, TextureDesc,
    WriteDescriptorSet,
};
use crate::{
    egui_renderer::EguiRenderer,
    egui_renderer::EguiRendererDesc,
    vkn::{Swapchain, VulkanContext, VulkanContextDesc},
    window::{WindowMode, WindowState, WindowStateDesc},
};
use ash::vk;
use egui::{Color32, RichText, Slider};
use gpu_allocator::vulkan::AllocatorCreateDesc;
use std::sync::{Arc, Mutex};
use winit::event::DeviceEvent;
use winit::{
    event::{ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
    window::WindowId,
};

pub struct InitializedApp {
    allocator: Allocator,
    egui_renderer: EguiRenderer,
    command_pool: CommandPool,
    command_buffer: CommandBuffer,
    window_state: WindowState,
    is_resize_pending: bool,
    swapchain: Swapchain,
    image_available_semaphore: Semaphore,
    render_finished_semaphore: Semaphore,
    fence: Fence,
    time_info: TimeInfo,
    slider_val: f32,

    compute_shader_module: ShaderModule,
    compute_pipeline: ComputePipeline,
    compute_descriptor_set: DescriptorSet,

    camera: Camera,

    descriptor_pool: DescriptorPool,
    renderer_resources: RendererResources,

    // note: always keep the context to end, as it has to be destroyed last
    vulkan_context: VulkanContext,
}

struct RendererResources {
    shader_write_tex: Texture,
    gui_input_buffer: Buffer,
    camera_info_buffer: Buffer,
}

impl RendererResources {
    fn create_shader_write_texture(
        screen_extent: [u32; 3],
        device: Device,
        allocator: Allocator,
    ) -> Texture {
        let tex_desc = TextureDesc {
            extent: screen_extent,
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();
        let tex = Texture::new(device, allocator, &tex_desc, &sam_desc);
        tex
    }

    fn new(
        device: Device,
        allocator: Allocator,
        compute_shader_module: &ShaderModule,
        screen_extent: &[u32; 2],
    ) -> Self {
        let shader_write_tex = Self::create_shader_write_texture(
            [screen_extent[0], screen_extent[1], 1],
            device.clone(),
            allocator.clone(),
        );

        let gui_input_layout = compute_shader_module.get_buffer_layout("GuiInput").unwrap();
        let gui_input_buffer = Buffer::new_sized(
            device.clone(),
            allocator.clone(),
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            gpu_allocator::MemoryLocation::CpuToGpu,
            gui_input_layout.get_size() as _,
        );

        let camera_info_layout = compute_shader_module
            .get_buffer_layout("CameraInfo")
            .unwrap();
        let camera_info_buffer = Buffer::new_sized(
            device.clone(),
            allocator,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            gpu_allocator::MemoryLocation::CpuToGpu,
            camera_info_layout.get_size() as _,
        );

        Self {
            shader_write_tex,
            gui_input_buffer,
            camera_info_buffer,
        }
    }

    fn on_resize(&mut self, device: Device, allocator: Allocator, screen_extent: &[u32; 2]) {
        self.shader_write_tex = Self::create_shader_write_texture(
            [screen_extent[0], screen_extent[1], 1],
            device,
            allocator,
        );
    }
}

impl InitializedApp {
    pub fn new(_event_loop: &ActiveEventLoop) -> Self {
        let window_state = Self::create_window_state(_event_loop);
        let vulkan_context = Self::create_vulkan_context(&window_state);

        let shader_compiler = ShaderCompiler::new().unwrap();

        let gpu_allocator = {
            let allocator_create_info = AllocatorCreateDesc {
                instance: vulkan_context.instance().as_raw().clone(),
                device: vulkan_context.device().as_raw().clone(),
                physical_device: vulkan_context.physical_device().as_raw(),
                debug_settings: Default::default(),
                buffer_device_address: false,
                allocation_sizes: Default::default(),
            };
            gpu_allocator::vulkan::Allocator::new(&allocator_create_info)
                .expect("Failed to create gpu allocator")
        };
        let allocator =
            Allocator::new(vulkan_context.device(), Arc::new(Mutex::new(gpu_allocator)));

        let swapchain = Swapchain::new(
            &vulkan_context,
            &window_state.window_size(),
            Default::default(),
        );

        let image_available_semaphore = Semaphore::new(vulkan_context.device());
        let render_finished_semaphore = Semaphore::new(vulkan_context.device());

        let fence = Fence::new(vulkan_context.device(), true);

        let command_pool = CommandPool::new(
            vulkan_context.device(),
            vulkan_context.queue_family_indices().general,
        );
        let command_buffer = CommandBuffer::new(vulkan_context.device(), &command_pool);

        let renderer = EguiRenderer::new(
            &vulkan_context,
            &window_state.window(),
            &allocator,
            &shader_compiler,
            swapchain.get_render_pass(),
            EguiRendererDesc {
                srgb_framebuffer: true,
                ..Default::default()
            },
        );

        let compute_shader_module = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/test.comp",
            "main",
        )
        .unwrap();

        let compute_pipeline =
            ComputePipeline::from_shader_module(vulkan_context.device(), &compute_shader_module);

        let descriptor_pool = DescriptorPool::from_descriptor_set_layouts(
            vulkan_context.device(),
            compute_pipeline
                .get_pipeline_layout()
                .get_descriptor_set_layouts(),
        )
        .unwrap();

        let screen_extent = window_state.window_size();

        let renderer_resources = RendererResources::new(
            vulkan_context.device().clone(),
            allocator.clone(),
            &compute_shader_module,
            &screen_extent,
        );

        let compute_descriptor_set = Self::create_compute_descriptor_set(
            descriptor_pool.clone(),
            &vulkan_context,
            &compute_pipeline,
            &renderer_resources,
        );

        let camera = Camera::new(
            glam::Vec3::ZERO,
            180.0,
            0.0,
            CameraDesc {
                movement: Default::default(),
                projection: Default::default(),
                aspect_ratio: screen_extent[0] as f32 / screen_extent[1] as f32,
            },
        );

        Self {
            vulkan_context,
            egui_renderer: renderer,
            window_state,

            compute_pipeline,
            compute_descriptor_set,

            compute_shader_module,

            allocator,
            command_pool,
            command_buffer,
            swapchain,
            image_available_semaphore,
            render_finished_semaphore,
            fence,

            descriptor_pool,
            renderer_resources,

            camera,
            is_resize_pending: false,
            time_info: TimeInfo::default(),
            slider_val: 0.0,
        }
    }

    fn create_compute_descriptor_set(
        descriptor_pool: DescriptorPool,
        vulkan_context: &VulkanContext,
        compute_pipeline: &ComputePipeline,
        renderer_resources: &RendererResources,
    ) -> DescriptorSet {
        let compute_descriptor_set = DescriptorSet::new(
            vulkan_context.device().clone(),
            compute_pipeline
                .get_pipeline_layout()
                .get_descriptor_set_layouts(),
            descriptor_pool,
        );
        compute_descriptor_set.perform_writes(&[
            WriteDescriptorSet::new_texture_write(
                0,
                vk::DescriptorType::STORAGE_IMAGE,
                &renderer_resources.shader_write_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_buffer_write(
                1,
                vk::DescriptorType::UNIFORM_BUFFER,
                &renderer_resources.gui_input_buffer,
            ),
            WriteDescriptorSet::new_buffer_write(
                2,
                vk::DescriptorType::UNIFORM_BUFFER,
                &renderer_resources.camera_info_buffer,
            ),
        ]);
        compute_descriptor_set
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
        self.vulkan_context.device().wait_idle();
        event_loop.exit();
    }

    /// Update the uniform buffers with the latest camera and debug values, called every frame
    fn update_uniform_buffers(
        compute_shader_module: &ShaderModule,
        camera: &Camera,
        renderer_resources: &mut RendererResources,
        debug_float: f32,
    ) {
        let gui_input_layout = compute_shader_module.get_buffer_layout("GuiInput").unwrap();
        let gui_input_data = BufferBuilder::from_layout(gui_input_layout)
            .set_float("debug_float", debug_float)
            .build();
        renderer_resources
            .gui_input_buffer
            .fill_raw(&gui_input_data)
            .unwrap();

        let camera_info_layout = compute_shader_module
            .get_buffer_layout("CameraInfo")
            .unwrap();

        let view_mat = camera.get_view_mat();
        let proj_mat = camera.get_proj_mat();
        let view_proj_mat = proj_mat * view_mat;
        let camera_info_data = BufferBuilder::from_layout(camera_info_layout)
            .set_vec4("camera_pos", camera.position_vec4().to_array())
            .set_mat4("view_mat", view_mat.to_cols_array_2d())
            .set_mat4("view_mat_inv", view_mat.inverse().to_cols_array_2d())
            .set_mat4("proj_mat", proj_mat.to_cols_array_2d())
            .set_mat4("proj_mat_inv", proj_mat.inverse().to_cols_array_2d())
            .set_mat4("view_proj_mat", view_proj_mat.to_cols_array_2d())
            .set_mat4(
                "view_proj_mat_inv",
                view_proj_mat.inverse().to_cols_array_2d(),
            )
            .build();
        renderer_resources
            .camera_info_buffer
            .fill_raw(&camera_info_data)
            .unwrap();
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
                self.camera.update_transform(self.time_info.delta_time());

                self.vulkan_context
                    .wait_for_fences(&[self.fence.as_raw()])
                    .unwrap();

                self.egui_renderer
                    .update(&self.command_pool, &self.window_state.window(), |ctx| {
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

                let device = self.vulkan_context.device();

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

                Self::update_uniform_buffers(
                    &self.compute_shader_module,
                    &self.camera,
                    &mut self.renderer_resources,
                    self.slider_val,
                );

                let cmdbuf = &self.command_buffer;
                cmdbuf.begin(false);

                self.renderer_resources
                    .shader_write_tex
                    .get_image()
                    .record_transition_barrier(cmdbuf, vk::ImageLayout::GENERAL);
                self.compute_pipeline.record_bind(cmdbuf);
                self.compute_pipeline.record_bind_descriptor_sets(
                    cmdbuf,
                    std::slice::from_ref(&self.compute_descriptor_set),
                    0,
                );
                self.compute_pipeline.record_dispatch(
                    cmdbuf,
                    [
                        self.window_state.window_size()[0],
                        self.window_state.window_size()[1],
                        1,
                    ],
                );

                self.swapchain.record_blit(
                    &self.renderer_resources.shader_write_tex.get_image(),
                    cmdbuf,
                    image_idx,
                );

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
                let command_buffers = [self.command_buffer.as_raw()];
                let submit_info = [vk::SubmitInfo::default()
                    .wait_semaphores(&wait_semaphores)
                    .wait_dst_stage_mask(&wait_stages)
                    .command_buffers(&command_buffers)
                    .signal_semaphores(&signal_semaphores)];

                unsafe {
                    self.vulkan_context
                        .device()
                        .as_raw()
                        .queue_submit(
                            self.vulkan_context.get_general_queue().as_raw(),
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
        match event {
            DeviceEvent::MouseMotion { delta } => {
                if !self.window_state.is_cursor_visible() {
                    self.camera.handle_mouse(&delta);
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
        self.vulkan_context.device().wait_idle();

        let window_size = self.window_state.window_size();

        self.camera.on_resize(&window_size);

        self.renderer_resources.on_resize(
            self.vulkan_context.device().clone(),
            self.allocator.clone(),
            &window_size,
        );
        self.descriptor_pool.reset().unwrap();
        self.compute_descriptor_set = Self::create_compute_descriptor_set(
            self.descriptor_pool.clone(),
            &self.vulkan_context,
            &self.compute_pipeline,
            &self.renderer_resources,
        );

        self.swapchain.on_resize(&window_size);

        // the render pass should be rebuilt when the swapchain is recreated
        self.egui_renderer
            .set_render_pass(self.swapchain.get_render_pass());

        self.is_resize_pending = false;
    }
}
