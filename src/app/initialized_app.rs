use crate::builder::Builder;
use crate::gameplay::{Camera, CameraDesc};
use crate::tracer::Tracer;
use crate::tree_gen::{Tree, TreeDesc};
use crate::util::ShaderCompiler;
use crate::util::TimeInfo;
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
    builder: Builder,

    // gui adjustables
    tree_pos: Vec3,
    tree_desc: TreeDesc,

    // note: always keep the context to end, as it has to be destroyed last
    vulkan_context: VulkanContext,
}

impl InitializedApp {
    pub fn new(_event_loop: &ActiveEventLoop) -> Self {
        let window_state = Self::create_window_state(_event_loop);
        let vulkan_context = Self::create_vulkan_context(&window_state);

        let shader_compiler = ShaderCompiler::new().unwrap();

        let device = vulkan_context.device();

        let gpu_allocator = {
            let allocator_create_info = AllocatorCreateDesc {
                instance: vulkan_context.instance().as_raw().clone(),
                device: device.as_raw().clone(),
                physical_device: vulkan_context.physical_device().as_raw(),
                debug_settings: Default::default(),
                buffer_device_address: false,
                allocation_sizes: Default::default(),
            };
            gpu_allocator::vulkan::Allocator::new(&allocator_create_info)
                .expect("Failed to create gpu allocator")
        };
        let allocator = Allocator::new(device, Arc::new(Mutex::new(gpu_allocator)));

        let swapchain = Swapchain::new(
            &vulkan_context,
            &window_state.window_size(),
            SwapchainDesc {
                present_mode: vk::PresentModeKHR::MAILBOX,
                ..Default::default()
            },
        );

        let image_available_semaphore = Semaphore::new(device);
        let render_finished_semaphore = Semaphore::new(device);

        let fence = Fence::new(device, true);

        let cmdbuf = CommandBuffer::new(device, vulkan_context.command_pool());

        let renderer = EguiRenderer::new(
            &vulkan_context,
            &window_state.window(),
            &allocator,
            &shader_compiler,
            swapchain.get_render_pass(),
        );

        let screen_extent = window_state.window_size();

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

        let chunk_dim = UVec3::new(3, 2, 3); // 2GB of Raw Data inside GPU is roughly 5^3 chunks of 256^3 voxels
        let mut builder = Builder::new(
            vulkan_context.clone(),
            allocator.clone(),
            &shader_compiler,
            UVec3::new(256, 256, 256),
            chunk_dim,
            chunk_dim,
            2 * 1024 * 1024 * 1024, // 2GB of octree buffer size
        );

        let tracer = Tracer::new(
            vulkan_context.clone(),
            allocator.clone(),
            &shader_compiler,
            &screen_extent,
            chunk_dim,
            builder.get_external_shared_resources(),
        );

        builder.init_chunks().unwrap();

        let tree_desc = TreeDesc::default();

        let mut this = Self {
            vulkan_context,
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
            builder,

            camera,
            is_resize_pending: false,
            time_info: TimeInfo::default(),

            tree_pos: Vec3::new(128.0, 30.0, 128.0),
            tree_desc,
        };
        this.init();
        return this;
    }

    fn init(&mut self) {
        self.add_tree();
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

                self.vulkan_context
                    .wait_for_fences(&[self.fence.as_raw()])
                    .unwrap();

                let mut tree_desc_changed = false;
                self.egui_renderer
                    .update(&self.window_state.window(), |ctx| {
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

                                    ui.add(egui::Label::new("Tree config"));

                                    ui.horizontal(|ui| {
                                        ui.label("Tree position");
                                        tree_desc_changed |= ui
                                            .add(
                                                egui::DragValue::new(&mut self.tree_pos.x)
                                                    .speed(1)
                                                    .prefix("x: "),
                                            )
                                            .changed();
                                        tree_desc_changed |= ui
                                            .add(
                                                egui::DragValue::new(&mut self.tree_pos.y)
                                                    .speed(1)
                                                    .prefix("y: "),
                                            )
                                            .changed();
                                        tree_desc_changed |= ui
                                            .add(
                                                egui::DragValue::new(&mut self.tree_pos.z)
                                                    .speed(1)
                                                    .prefix("z: "),
                                            )
                                            .changed();
                                    });

                                    tree_desc_changed |= ui
                                        .add(
                                            egui::Slider::new(&mut self.tree_desc.size, 0.0..=5.0)
                                                .text("Size"),
                                        )
                                        .changed();

                                    tree_desc_changed |= ui
                                        .add(
                                            egui::Slider::new(
                                                &mut self.tree_desc.trunk_thickness,
                                                0.0..=4.0,
                                            )
                                            .text("Trunk Thickness"),
                                        )
                                        .changed();
                                    tree_desc_changed |= ui
                                        .add(
                                            egui::Slider::new(
                                                &mut self.tree_desc.spread,
                                                0.0..=1.0,
                                            )
                                            .text("Spread"),
                                        )
                                        .changed();
                                    tree_desc_changed |= ui
                                        .add(
                                            egui::Slider::new(
                                                &mut self.tree_desc.twisted,
                                                0.0..=1.0,
                                            )
                                            .text("Twisted"),
                                        )
                                        .changed();
                                    tree_desc_changed |= ui
                                        .add(
                                            egui::Slider::new(
                                                &mut self.tree_desc.leaves_size,
                                                0.0..=10.0,
                                            )
                                            .text("Leaves size"),
                                        )
                                        .changed();
                                    tree_desc_changed |= ui
                                        .add(
                                            egui::Slider::new(
                                                &mut self.tree_desc.gravity,
                                                0.0..=1.0,
                                            )
                                            .text("Gravity"),
                                        )
                                        .changed();
                                    tree_desc_changed |= ui
                                        .add(
                                            egui::Slider::new(
                                                &mut self.tree_desc.iterations,
                                                1..=20,
                                            )
                                            .text("Iterations"),
                                        )
                                        .changed();
                                    tree_desc_changed |= ui
                                        .add(
                                            egui::Slider::new(&mut self.tree_desc.wide, 0.0..=1.0)
                                                .text("Wide"),
                                        )
                                        .changed();
                                    tree_desc_changed |= ui
                                        .add(
                                            egui::Slider::new(&mut self.tree_desc.seed, 0..=100)
                                                .text("Seed"),
                                        )
                                        .changed();

                                    tree_desc_changed |=
                                        ui.add(egui::Button::new("Instantiate!")).clicked()
                                });
                            });
                    });

                if tree_desc_changed {
                    self.add_tree();
                }

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

                self.tracer.update_buffers(&self.camera);

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

    /// Add a tree to the scene using the current tree description and position.
    fn add_tree(&mut self) {
        let tree = Tree::new(self.tree_desc.clone());
        let result = self.builder.add_tree(&tree, self.tree_pos);
        self.builder.create_scene_bvh(&tree, self.tree_pos);
        if let Err(err) = result {
            println!("Failed to add tree: {}", err);
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
        self.vulkan_context.device().wait_idle();

        let window_size = self.window_state.window_size();

        self.camera.on_resize(&window_size);
        self.tracer
            .on_resize(&window_size, self.builder.get_external_shared_resources());
        self.swapchain.on_resize(&window_size);

        // the render pass should be rebuilt when the swapchain is recreated
        self.egui_renderer
            .set_render_pass(self.swapchain.get_render_pass());

        self.is_resize_pending = false;
    }
}
