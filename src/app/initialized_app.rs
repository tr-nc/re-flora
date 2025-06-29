#[allow(unused)]
use crate::util::Timer;

use crate::builder::{ContreeBuilder, PlainBuilder, SceneAccelBuilder, SurfaceBuilder};
use crate::geom::{build_bvh, UAabb3};
use crate::procedual_placer::{generate_positions, PlacerDesc};
use crate::tracer::{Tracer, TracerDesc};
use crate::tree_gen::{Tree, TreeDesc};
use crate::util::{get_sun_dir, ShaderCompiler};
use crate::util::{TimeInfo, BENCH};
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
use std::time::Instant;
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

    tracer: Tracer,

    // builders
    plain_builder: PlainBuilder,
    surface_builder: SurfaceBuilder,
    contree_builder: ContreeBuilder,
    scene_accel_builder: SceneAccelBuilder,

    // gui adjustables
    debug_float: f32,
    debug_bool: bool,
    sun_altitude: f32,
    sun_azimuth: f32,
    sun_size: f32,
    sun_color: egui::Color32,

    tree_pos: Vec3,

    tree_desc: TreeDesc,
    prev_bound: UAabb3,

    // note: always keep the context to end, as it has to be destroyed last
    vulkan_ctx: VulkanContext,
}

const VOXEL_DIM_PER_CHUNK: UVec3 = UVec3::new(256, 256, 256);
const CHUNK_DIM: UVec3 = UVec3::new(5, 2, 5);
const FREE_ATLAS_DIM: UVec3 = UVec3::new(512, 512, 512);

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
            window_state.window_extent(),
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
            vulkan_ctx.clone(),
            &window_state.window(),
            allocator.clone(),
            &shader_compiler,
            swapchain.get_render_pass(),
        );

        let mut plain_builder = PlainBuilder::new(
            vulkan_ctx.clone(),
            &shader_compiler,
            allocator.clone(),
            CHUNK_DIM * VOXEL_DIM_PER_CHUNK,
            FREE_ATLAS_DIM,
        );

        let mut surface_builder = SurfaceBuilder::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &shader_compiler,
            plain_builder.get_resources(),
            VOXEL_DIM_PER_CHUNK,
            VOXEL_DIM_PER_CHUNK.x as u64
                * VOXEL_DIM_PER_CHUNK.z as u64
                * CHUNK_DIM.x as u64
                * CHUNK_DIM.y as u64
                * CHUNK_DIM.z as u64,
        );

        let mut contree_builder = ContreeBuilder::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &shader_compiler,
            surface_builder.get_resources(),
            VOXEL_DIM_PER_CHUNK,
            512 * 1024 * 1024, // node buffer pool size
            512 * 1024 * 1024, // leaf buffer pool size
        );

        let mut scene_accel_builder = SceneAccelBuilder::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &shader_compiler,
            CHUNK_DIM,
        );

        Self::init(
            &mut plain_builder,
            &mut surface_builder,
            &mut contree_builder,
            &mut scene_accel_builder,
        );

        let tracer = Tracer::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &shader_compiler,
            CHUNK_DIM,
            window_state.window_extent(),
            &contree_builder.get_resources().node_data,
            &contree_builder.get_resources().leaf_data,
            &scene_accel_builder.get_resources().scene_offset_tex,
            TracerDesc {
                scaling_factor: 0.5,
            },
        );

        return Self {
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
            surface_builder,
            contree_builder,
            scene_accel_builder,

            is_resize_pending: false,
            time_info: TimeInfo::default(),

            debug_float: 0.0,
            debug_bool: true,
            sun_altitude: 14.0,
            sun_azimuth: 280.0,
            sun_size: 0.02,
            sun_color: egui::Color32::from_rgb(255, 233, 144),
            tree_pos: Vec3::new(512.0, 250.0, 512.0),
            tree_desc: TreeDesc::default(),
            prev_bound: Default::default(),
        };
    }

    fn init(
        plain_builder: &mut PlainBuilder,
        surface_builder: &mut SurfaceBuilder,
        contree_builder: &mut ContreeBuilder,
        scene_accel_builder: &mut SceneAccelBuilder,
    ) {
        plain_builder.chunk_init(UVec3::new(0, 0, 0), VOXEL_DIM_PER_CHUNK * CHUNK_DIM);

        let chunk_pos_to_build_min = UVec3::new(0, 0, 0);
        let chunk_pos_to_build_max = CHUNK_DIM;

        for x in chunk_pos_to_build_min.x..chunk_pos_to_build_max.x {
            for y in chunk_pos_to_build_min.y..chunk_pos_to_build_max.y {
                for z in chunk_pos_to_build_min.z..chunk_pos_to_build_max.z {
                    let chunk_idx = UVec3::new(x, y, z);
                    let this_bound = UAabb3::new(
                        chunk_idx * VOXEL_DIM_PER_CHUNK,
                        (chunk_idx + UVec3::ONE) * VOXEL_DIM_PER_CHUNK - UVec3::ONE,
                    );
                    Self::mesh_regenerate(
                        surface_builder,
                        contree_builder,
                        scene_accel_builder,
                        this_bound,
                    );
                }
            }
        }

        BENCH.lock().unwrap().summary();
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
        self.vulkan_ctx.device().wait_idle();
        event_loop.exit();
    }

    fn add_a_tree(&mut self, tree_desc: TreeDesc, tree_pos: Vec3, clean_up_before_add: bool) {
        let tree = Tree::new(tree_desc);
        let mut round_cones = Vec::new();
        for tree_trunk in tree.trunks() {
            let mut round_cone = tree_trunk.clone();
            round_cone.transform(tree_pos);
            round_cones.push(round_cone);
        }

        let mut leaves_data_sequential = vec![0; round_cones.len()];
        for i in 0..round_cones.len() {
            leaves_data_sequential[i] = i as u32;
        }
        let mut aabbs = Vec::new();
        for round_cone in &round_cones {
            aabbs.push(round_cone.aabb());
        }
        let bvh_nodes = build_bvh(&aabbs, &leaves_data_sequential).unwrap();

        let this_bound = UAabb3::new(bvh_nodes[0].aabb.min_uvec3(), bvh_nodes[0].aabb.max_uvec3());

        if clean_up_before_add {
            self.plain_builder.chunk_init(
                self.prev_bound.min(),
                self.prev_bound.max() - self.prev_bound.min(),
            );
        }

        self.plain_builder.chunk_modify(&bvh_nodes, &round_cones);
        Self::mesh_regenerate(
            &mut self.surface_builder,
            &mut self.contree_builder,
            &mut self.scene_accel_builder,
            this_bound.union(&self.prev_bound),
        );

        self.prev_bound = this_bound;
    }

    fn mesh_regenerate(
        surface_builder: &mut SurfaceBuilder,
        contree_builder: &mut ContreeBuilder,
        scene_accel_builder: &mut SceneAccelBuilder,
        bound: UAabb3,
    ) {
        let affected_chunk_indices = get_affected_chunk_indices(bound.min(), bound.max());

        for chunk_idx in affected_chunk_indices {
            let atlas_offset = chunk_idx * VOXEL_DIM_PER_CHUNK;

            let now = Instant::now();
            let active_voxel_len = surface_builder.build_surface(atlas_offset);
            BENCH.lock().unwrap().record("build_surface", now.elapsed());

            if active_voxel_len == 0 {
                log::debug!("Don't need to build contree because the chunk is empty");
                continue;
            }

            let now = Instant::now();
            let res = contree_builder.build_and_alloc(atlas_offset).unwrap();
            BENCH
                .lock()
                .unwrap()
                .record("build_and_alloc", now.elapsed());

            if let Some(res) = res {
                let (node_buffer_offset, leaf_buffer_offset) = res;
                scene_accel_builder.update_scene_tex(
                    chunk_idx,
                    node_buffer_offset,
                    leaf_buffer_offset,
                );
            } else {
                log::debug!("Don't need to update scene tex because the chunk is empty");
            }
        }

        fn get_affected_chunk_indices(min_bound: UVec3, max_bound: UVec3) -> Vec<UVec3> {
            let min_chunk_idx = min_bound / VOXEL_DIM_PER_CHUNK;
            let max_chunk_idx = max_bound / VOXEL_DIM_PER_CHUNK;

            let mut affacted = Vec::new();
            for x in min_chunk_idx.x..=max_chunk_idx.x {
                for y in min_chunk_idx.y..=max_chunk_idx.y {
                    for z in min_chunk_idx.z..=max_chunk_idx.z {
                        affacted.push(UVec3::new(x, y, z));
                    }
                }
            }
            return affacted;
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
                    self.tracer.handle_keyboard(&event);
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

                    self.tracer.handle_mouse(self.smoothed_mouse_delta);
                }

                self.tracer.update_transform(frame_delta_time);

                self.vulkan_ctx
                    .wait_for_fences(&[self.fence.as_raw()])
                    .unwrap();

                let mut grass_changed = false;
                let mut add_tree_requested = false;
                let mut tree_desc_changed = false;

                self.egui_renderer
                    .update(&self.window_state.window(), |ctx| {
                        let my_frame = egui::containers::Frame {
                            fill: Color32::from_rgba_premultiplied(115, 34, 85, 250),
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

                                    grass_changed |= ui
                                        .add(
                                            egui::Slider::new(&mut self.debug_float, 0.0..=1.0)
                                                .text("Debug Float"),
                                        )
                                        .changed();

                                    ui.add(egui::Checkbox::new(&mut self.debug_bool, "Debug Bool"));

                                    ui.add(
                                        egui::Slider::new(&mut self.sun_altitude, 0.0..=180.0)
                                            .text("Sun Altitude (degrees above horizon)"),
                                    );

                                    ui.add(
                                        egui::Slider::new(&mut self.sun_azimuth, 0.0..=360.0)
                                            .text("Sun Azimuth (degrees around Y axis)"),
                                    );

                                    ui.add(
                                        egui::Slider::new(&mut self.sun_size, 0.0..=1.0)
                                            .text("Sun Size (relative to screen)"),
                                    );

                                    ui.add(egui::Label::new("Sun Color:"));
                                    ui.color_edit_button_srgba(&mut self.sun_color);

                                    // a button to add a new tree
                                    if ui.button("Add New Tree").clicked() {
                                        add_tree_requested = true;
                                    }

                                    ui.separator();

                                    ui.heading("Tree Position");

                                    tree_desc_changed |= ui
                                        .add(
                                            egui::Slider::new(&mut self.tree_pos.x, 0.0..=512.0)
                                                .text("New Tree X Position"),
                                        )
                                        .changed();
                                    tree_desc_changed |= ui
                                        .add(
                                            egui::Slider::new(&mut self.tree_pos.y, 0.0..=512.0)
                                                .text("New Tree Y Position"),
                                        )
                                        .changed();
                                    tree_desc_changed |= ui
                                        .add(
                                            egui::Slider::new(&mut self.tree_pos.z, 0.0..=512.0)
                                                .text("New Tree Z Position"),
                                        )
                                        .changed();

                                    ui.heading("Tree Descriptor"); // Heading for the new section

                                    tree_desc_changed |= self.tree_desc.edit_by_gui(ui);
                                });
                            });
                    });

                if tree_desc_changed {
                    self.add_a_tree(
                        self.tree_desc.clone(),
                        self.tree_pos,
                        true, // clean up before adding a new tree
                    );
                }

                if add_tree_requested {
                    let placer_desc = PlacerDesc::new(42);
                    let map_dim = VOXEL_DIM_PER_CHUNK * CHUNK_DIM;
                    let map_dim_2d = Vec2::new(map_dim.x as f32, map_dim.z as f32);
                    let generated_positions = generate_positions(map_dim_2d, 128.0, &placer_desc);

                    log::debug!(
                        "Generated {} positions for trees",
                        generated_positions.len()
                    );
                    for pos in generated_positions {
                        let tree_pos = Vec3::new(pos.x, 256.0, pos.y);
                        self.add_a_tree(self.tree_desc.clone(), tree_pos, false);
                    }
                }

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

                let cmdbuf = &self.cmdbuf;
                cmdbuf.begin(false);

                self.tracer
                    .update_buffers_and_record(
                        cmdbuf,
                        &self.time_info,
                        &self.surface_builder.get_resources(),
                        self.surface_builder.get_grass_instance_len(),
                        self.debug_float,
                        self.debug_bool,
                        get_sun_dir(self.sun_altitude, self.sun_azimuth),
                        self.sun_size,
                        Vec3::new(
                            self.sun_color.r() as f32,
                            self.sun_color.g() as f32,
                            self.sun_color.b() as f32,
                        ),
                    )
                    .unwrap();

                self.swapchain.record_blit(
                    self.tracer.get_screen_output_tex().get_image(),
                    cmdbuf,
                    image_idx,
                );

                let render_area = self.window_state.window_extent();

                self.swapchain
                    .record_begin_render_pass_cmdbuf(cmdbuf, image_idx, render_area);

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

        let window_extent = self.window_state.window_extent();

        self.swapchain.on_resize(window_extent);
        self.tracer.on_resize(window_extent);

        // the render pass should be rebuilt when the swapchain is recreated
        self.egui_renderer
            .set_render_pass(self.swapchain.get_render_pass());

        self.is_resize_pending = false;
    }
}
