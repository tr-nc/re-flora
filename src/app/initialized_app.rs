use crate::gameplay::Camera;
use crate::util::compiler::ShaderCompiler;
use crate::util::time_info::TimeInfo;
use crate::vkn::{
    execute_one_time_command, Allocator, Buffer, BufferBuilder, CommandBuffer, CommandPool,
    ComputePipeline, DescriptorPool, DescriptorSet, Fence, Semaphore, ShaderModule, Texture,
    TextureDesc, WriteDescriptorSet,
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
    renderer: EguiRenderer,
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

    camera: Camera,

    // note: keep it at the end, it has to be destroyed last
    vulkan_context: VulkanContext,
}

impl InitializedApp {
    pub fn new(_event_loop: &ActiveEventLoop) -> Self {
        let window_state = Self::create_window_state(_event_loop);
        let vulkan_context = Self::create_vulkan_context(&window_state);

        let shader_compiler = ShaderCompiler::new(Default::default()).unwrap();

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
        let mut allocator =
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

        // compute shader test

        let compute_shader_module = ShaderModule::from_glsl(
            vulkan_context.device(),
            &shader_compiler,
            "shader/test.comp",
            "main",
        )
        .unwrap();

        let test_input_layout = compute_shader_module
            .get_buffer_layout("TestInput")
            .unwrap();

        log::debug!("Test Input Layout: {:?}", test_input_layout);

        let test_input_data = BufferBuilder::with_layout(test_input_layout)
            .set_float("aaa", 1.0)
            .set_uint("bbb", 222)
            .build();
        log::debug!("Test Input Data: {:?}", test_input_data);

        let test_input_buffer = Buffer::new_sized(
            vulkan_context.device(),
            &mut allocator,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
            test_input_data.len(),
        );

        let compute_pipeline =
            ComputePipeline::from_shader_module(vulkan_context.device(), &compute_shader_module);

        let descriptor_set_layouts = compute_pipeline
            .get_pipeline_layout()
            .get_descriptor_set_layouts();

        let descriptor_pool = DescriptorPool::from_descriptor_set_layouts(
            vulkan_context.device(),
            descriptor_set_layouts,
        )
        .unwrap();

        let screen_extent = window_state.window_size();
        let screen_extent = [screen_extent[0] as u32, screen_extent[1] as u32, 1];
        log::info!("Screen extent: {:?}", screen_extent);

        let tex_desc_1 = TextureDesc {
            extent: screen_extent,
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let tex_desc_2 = TextureDesc {
            extent: screen_extent,
            format: vk::Format::R8G8B8A8_UNORM,
            usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            initial_layout: vk::ImageLayout::UNDEFINED,
            aspect: vk::ImageAspectFlags::COLOR,
            ..Default::default()
        };
        let sam_desc = Default::default();

        let shader_write_tex =
            Texture::new(vulkan_context.device(), &allocator, &tex_desc_1, &sam_desc);
        let test_cpy_dst_tex =
            Texture::new(vulkan_context.device(), &allocator, &tex_desc_2, &sam_desc);

        let set = DescriptorSet::new(
            &vulkan_context.device(),
            descriptor_set_layouts,
            &descriptor_pool,
        );
        let mut write_ds = WriteDescriptorSet::new(0, vk::DescriptorType::STORAGE_IMAGE);
        write_ds.add_texture(&shader_write_tex, vk::ImageLayout::GENERAL);
        set.perform_writes(&[write_ds]);

        execute_one_time_command(
            vulkan_context.device(),
            &command_pool,
            &vulkan_context.get_general_queue(),
            |cmdbuf| {
                shader_write_tex
                    .get_image()
                    .record_transition_barrier(cmdbuf, vk::ImageLayout::GENERAL);

                compute_pipeline.record_bind(cmdbuf);
                compute_pipeline.record_bind_descriptor_sets(cmdbuf, &[set], 0);
                compute_pipeline.record_dispatch(cmdbuf, screen_extent);

                shader_write_tex
                    .get_image()
                    .record_copy_to(cmdbuf, &test_cpy_dst_tex.get_image())
                    .unwrap();
            },
        );

        Self {
            vulkan_context,
            renderer,
            window_state,

            allocator,
            command_pool,
            command_buffer,
            swapchain,
            image_available_semaphore,
            render_finished_semaphore,
            fence,

            camera: Default::default(),
            is_resize_pending: false,
            time_info: TimeInfo::default(),
            slider_val: 0.0,
        }
    }

    fn create_window_state(event_loop: &ActiveEventLoop) -> WindowState {
        let window_descriptor = WindowStateDesc {
            title: "Re: Flora".to_owned(),
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
            VulkanContextDesc {
                name: "Re: Flora".into(),
            },
        )
    }

    pub fn on_terminate(&mut self, event_loop: &ActiveEventLoop) {
        // ensure all command buffers are done executing before terminating anything
        self.vulkan_context.wait_device_idle().unwrap();
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
                .renderer
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

                self.renderer
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
                        .device()
                        .as_raw()
                        .reset_fences(&[self.fence.as_raw()])
                        .expect("Failed to reset fences")
                };

                let render_area = vk::Extent2D {
                    width: self.window_state.window_size()[0],
                    height: self.window_state.window_size()[1],
                };

                self.renderer.record_command_buffer(
                    &self.vulkan_context.device(),
                    &self.swapchain,
                    &self.command_pool,
                    &self.command_buffer,
                    image_index,
                    render_area,
                );

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
        // Resize the window here

        let window_size = self.window_state.window_size();

        self.swapchain.on_resize(&self.vulkan_context, &window_size);

        // the render pass is rebuilt when the swapchain is recreated
        self.renderer
            .set_render_pass(self.swapchain.get_render_pass());

        self.is_resize_pending = false;
    }
}
