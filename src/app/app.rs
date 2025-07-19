#[allow(unused)]
use crate::util::Timer;

use crate::audio::{AudioEngine, ClipCache, PlayMode, SoundDataConfig};
use crate::builder::{ContreeBuilder, PlainBuilder, SceneAccelBuilder, SurfaceBuilder};
use crate::geom::{build_bvh, UAabb3};
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
use anyhow::Result;
use ash::vk;
use egui::{Color32, RichText};
use glam::{UVec3, Vec2, Vec3};
use gpu_allocator::vulkan::AllocatorCreateDesc;
use kira::{StartTime, Tween};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::event::DeviceEvent;
use winit::{
    event::{ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
    window::WindowId,
};

pub struct App {
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
    debug_uint: u32,
    sun_altitude: f32,
    sun_azimuth: f32,
    sun_size: f32,
    sun_color: egui::Color32,
    sun_luminance: f32,
    ambient_light: egui::Color32,
    temporal_position_phi: f32,
    temporal_alpha: f32,
    phi_c: f32,
    phi_n: f32,
    phi_p: f32,
    min_phi_z: f32,
    max_phi_z: f32,
    phi_z_stable_sample_count: f32,
    is_changing_lum_phi: bool,
    is_spatial_denoising_skipped: bool,
    is_taa_enabled: bool,
    tree_pos: Vec3,
    config_panel_visible: bool,
    is_fly_mode: bool,

    tree_desc: TreeDesc,
    prev_bound: UAabb3,

    // starlight parameters
    starlight_iterations: i32,
    starlight_formuparam: f32,
    starlight_volsteps: i32,
    starlight_stepsize: f32,
    starlight_zoom: f32,
    starlight_tile: f32,
    starlight_speed: f32,
    starlight_brightness: f32,
    starlight_darkmatter: f32,
    starlight_distfading: f32,
    starlight_saturation: f32,

    // note: always keep the context to end, as it has to be destroyed last
    vulkan_ctx: VulkanContext,

    #[allow(dead_code)]
    audio_engine: AudioEngine,
}

const VOXEL_DIM_PER_CHUNK: UVec3 = UVec3::new(256, 256, 256);
const CHUNK_DIM: UVec3 = UVec3::new(5, 2, 5);
const FREE_ATLAS_DIM: UVec3 = UVec3::new(512, 512, 512);

impl App {
    pub fn new(_event_loop: &ActiveEventLoop) -> Result<Self> {
        let chunk_bound = UAabb3::new(UVec3::ZERO, CHUNK_DIM);
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
            chunk_bound,
            VOXEL_DIM_PER_CHUNK.x as u64 * VOXEL_DIM_PER_CHUNK.z as u64,
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
            chunk_bound,
        )?;

        Self::init(
            &mut plain_builder,
            &mut surface_builder,
            &mut contree_builder,
            &mut scene_accel_builder,
        )?;

        let mut audio_engine = AudioEngine::new()?;

        let tracer = Tracer::new(
            vulkan_ctx.clone(),
            allocator.clone(),
            &shader_compiler,
            chunk_bound,
            window_state.window_extent(),
            contree_builder.get_resources(),
            scene_accel_builder.get_resources(),
            TracerDesc {
                scaling_factor: 0.5,
            },
            audio_engine.clone(),
        )?;

        Self::add_ambient_sounds(&mut audio_engine)?;

        let mut app = Self {
            vulkan_ctx,
            egui_renderer: renderer,
            window_state,

            accumulated_mouse_delta: Vec2::ZERO,
            smoothed_mouse_delta: Vec2::ZERO,

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
            debug_uint: 0,
            temporal_position_phi: 0.8,
            temporal_alpha: 0.04,
            phi_c: 0.75,
            phi_n: 20.0,
            phi_p: 0.05,
            min_phi_z: 0.0,
            max_phi_z: 0.5,
            phi_z_stable_sample_count: 0.05,
            is_changing_lum_phi: true,
            is_spatial_denoising_skipped: false,
            is_taa_enabled: false,
            sun_altitude: 0.25,
            sun_azimuth: 0.8,
            sun_size: 0.1,
            sun_color: egui::Color32::from_rgb(255, 233, 144),
            sun_luminance: 1.0,
            ambient_light: egui::Color32::from_rgb(25, 25, 25),
            tree_pos: Vec3::new(512.0, 0.0, 512.0),
            tree_desc: TreeDesc::default(),
            prev_bound: Default::default(),
            config_panel_visible: false,
            is_fly_mode: true,

            starlight_iterations: 18,
            starlight_formuparam: 0.5,
            starlight_volsteps: 10,
            starlight_stepsize: 0.12,
            starlight_zoom: 0.88,
            starlight_tile: 1.1,
            starlight_speed: 0.01,
            starlight_brightness: 0.001,
            starlight_darkmatter: 0.8,
            starlight_distfading: 0.885,
            starlight_saturation: 1.0,

            audio_engine,
        };

        app.add_a_tree(app.tree_desc.clone(), app.tree_pos, true)?;
        Ok(app)
    }

    fn add_ambient_sounds(audio_engine: &mut AudioEngine) -> Result<()> {
        let leaf_rustling_sound = "assets/sfx/leaf_rustling.wav";
        let mut _leaf_rustling_clip_cache = ClipCache::from_files(
            &[leaf_rustling_sound],
            SoundDataConfig {
                mode: PlayMode::Loop,
                volume: -30.0,
                fade_in_tween: Some(Tween {
                    start_time: StartTime::Immediate,
                    duration: Duration::from_secs(2),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )?;

        let wind_ambient_sound = "assets/sfx/WINDDsgn_Wind, Gentle, Designed 03_SARM_Wind.wav";
        let mut wind_ambient_clip_cache = ClipCache::from_files(
            &[wind_ambient_sound],
            SoundDataConfig {
                mode: PlayMode::Loop,
                volume: -20.0,
                fade_in_tween: Some(Tween {
                    start_time: StartTime::Immediate,
                    duration: Duration::from_secs(2),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )?;

        // audio_engine.play(&leaf_rustling_clip_cache.next())?;
        audio_engine.play(&wind_ambient_clip_cache.next())?;

        Ok(())
    }

    fn init(
        plain_builder: &mut PlainBuilder,
        surface_builder: &mut SurfaceBuilder,
        contree_builder: &mut ContreeBuilder,
        scene_accel_builder: &mut SceneAccelBuilder,
    ) -> Result<()> {
        plain_builder.chunk_init(UVec3::new(0, 0, 0), VOXEL_DIM_PER_CHUNK * CHUNK_DIM)?;

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
                    Self::mesh_generate(
                        surface_builder,
                        contree_builder,
                        scene_accel_builder,
                        this_bound,
                    )?;
                }
            }
        }

        BENCH.lock().unwrap().summary();
        Ok(())
    }

    fn create_window_state(event_loop: &ActiveEventLoop) -> WindowState {
        const WINDOW_TITLE_DEBUG: &str = "Re: Flora - debug build";
        const WINDOW_TITLE_RELEASE: &str = "Re: Flora - release build";
        let using_mode = if cfg!(debug_assertions) {
            WINDOW_TITLE_DEBUG
        } else {
            WINDOW_TITLE_RELEASE
        };
        let window_descriptor = WindowStateDesc {
            title: using_mode.to_owned(),
            window_mode: WindowMode::Windowed(false),
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

    fn add_a_tree(
        &mut self,
        tree_desc: TreeDesc,
        tree_pos: Vec3,
        clean_up_before_add: bool,
    ) -> Result<()> {
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
            )?;
        }

        self.plain_builder.chunk_modify(&bvh_nodes, &round_cones)?;
        Self::mesh_generate(
            &mut self.surface_builder,
            &mut self.contree_builder,
            &mut self.scene_accel_builder,
            this_bound.union_with(&self.prev_bound),
        )?;

        self.prev_bound = this_bound;

        Ok(())
    }

    fn mesh_generate(
        surface_builder: &mut SurfaceBuilder,
        contree_builder: &mut ContreeBuilder,
        scene_accel_builder: &mut SceneAccelBuilder,
        bound: UAabb3,
    ) -> Result<()> {
        let affected_chunk_indices = get_affected_chunk_indices(bound.min(), bound.max());

        for chunk_id in affected_chunk_indices {
            let atlas_offset = chunk_id * VOXEL_DIM_PER_CHUNK;

            let now = Instant::now();
            let res = surface_builder.build_surface(chunk_id);
            if let Err(e) = res {
                log::error!("Failed to build surface for chunk {}: {}", chunk_id, e);
                continue;
            }
            // we don't use the active_voxel_len here

            BENCH.lock().unwrap().record("build_surface", now.elapsed());

            let now = Instant::now();
            let res = contree_builder.build_and_alloc(atlas_offset).unwrap();
            BENCH
                .lock()
                .unwrap()
                .record("build_and_alloc", now.elapsed());

            if let Some(res) = res {
                let (node_buffer_offset, leaf_buffer_offset) = res;
                scene_accel_builder.update_scene_tex(
                    chunk_id,
                    node_buffer_offset,
                    leaf_buffer_offset,
                )?;
            } else {
                log::debug!("Don't need to update scene tex because the chunk is empty");
            }
        }
        return Ok(());

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
                    self.config_panel_visible = !self.config_panel_visible;
                    if self.config_panel_visible {
                        self.window_state.set_cursor_visibility(true);
                        self.window_state.set_cursor_grab(false);
                    } else {
                        self.window_state.set_cursor_visibility(false);
                        self.window_state.set_cursor_grab(true);
                    }
                }

                if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyF {
                    self.window_state.toggle_fullscreen();
                }

                if event.state == ElementState::Pressed && event.physical_key == KeyCode::KeyG {
                    self.is_fly_mode = !self.is_fly_mode;
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
                    self.accumulated_mouse_delta = Vec2::ZERO;

                    let alpha = 0.4; // mouse smoothing factor: 0 = no smoothing, 1 = infinite smoothing
                    self.smoothed_mouse_delta =
                        self.smoothed_mouse_delta * alpha + mouse_delta * (1.0 - alpha);

                    self.tracer.handle_mouse(self.smoothed_mouse_delta);
                }

                let mut tree_desc_changed = false;
                self.egui_renderer
                    .update(&self.window_state.window(), |ctx| {
                        let mut style = (*ctx.style()).clone();
                        style.visuals.override_text_color = Some(egui::Color32::WHITE);
                        ctx.set_style(style);

                        // Config panel - only show if visible
                        if self.config_panel_visible {
                            let config_frame = egui::containers::Frame {
                                fill: Color32::from_rgba_premultiplied(123, 64, 25, 250),
                                inner_margin: egui::Margin::same(10),
                                ..Default::default()
                            };

                            egui::SidePanel::left("config_panel")
                                .frame(config_frame)
                                .resizable(true)
                                .default_width(320.0)
                                .min_width(250.0)
                                .show(ctx, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.heading("Configuration");
                                    });

                                    ui.separator();

                                    egui::ScrollArea::vertical().show(ui, |ui| {
                                        ui.collapsing("Debug Settings", |ui| {
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.debug_float,
                                                    0.0..=10.0,
                                                )
                                                .text("Debug Float"),
                                            );
                                            ui.add(
                                                egui::Slider::new(&mut self.debug_uint, 0..=100)
                                                    .text("Debug UInt"),
                                            );
                                            ui.add(egui::Checkbox::new(
                                                &mut self.debug_bool,
                                                "Debug Bool",
                                            ));
                                        });

                                        ui.collapsing("Sky Settings", |ui| {
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.sun_altitude,
                                                    -1.0..=1.0,
                                                )
                                                .text("Altitude (normalized)")
                                                .smart_aim(false),
                                            );
                                            ui.add(
                                                egui::Slider::new(&mut self.sun_azimuth, 0.0..=1.0)
                                                    .text("Azimuth (normalized)"),
                                            );
                                            ui.add(
                                                egui::Slider::new(&mut self.sun_size, 0.0..=1.0)
                                                    .text("Size (relative)"),
                                            );
                                            ui.horizontal(|ui| {
                                                ui.label("Sun Color:");
                                                ui.color_edit_button_srgba(&mut self.sun_color);
                                            });
                                            ui.horizontal(|ui| {
                                                ui.add(
                                                    egui::Slider::new(
                                                        &mut self.sun_luminance,
                                                        0.0..=10.0,
                                                    )
                                                    .text("Sun Luminance"),
                                                );
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label("Ambient Light:");
                                                ui.color_edit_button_srgba(&mut self.ambient_light);
                                            });
                                        });

                                        ui.collapsing("Starlight Settings", |ui| {
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.starlight_iterations,
                                                    1..=30,
                                                )
                                                .text("Iterations"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.starlight_formuparam,
                                                    0.0..=1.0,
                                                )
                                                .text("Form Parameter"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.starlight_volsteps,
                                                    1..=50,
                                                )
                                                .text("Volume Steps"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.starlight_stepsize,
                                                    0.01..=1.0,
                                                )
                                                .text("Step Size"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.starlight_zoom,
                                                    0.1..=2.0,
                                                )
                                                .text("Zoom"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.starlight_tile,
                                                    0.1..=2.0,
                                                )
                                                .text("Tile"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.starlight_speed,
                                                    0.001..=0.1,
                                                )
                                                .text("Speed"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.starlight_brightness,
                                                    0.0001..=0.01,
                                                )
                                                .text("Brightness"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.starlight_darkmatter,
                                                    0.0..=1.0,
                                                )
                                                .text("Dark Matter"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.starlight_distfading,
                                                    0.0..=1.0,
                                                )
                                                .text("Distance Fading"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.starlight_saturation,
                                                    0.0..=1.0,
                                                )
                                                .text("Saturation"),
                                            );
                                        });

                                        ui.collapsing("Tree Settings", |ui| {
                                            ui.label("Position:");
                                            tree_desc_changed |= ui
                                                .add(
                                                    egui::Slider::new(
                                                        &mut self.tree_pos.x,
                                                        0.0..=1024.0,
                                                    )
                                                    .text("X"),
                                                )
                                                .changed();
                                            tree_desc_changed |= ui
                                                .add(
                                                    egui::Slider::new(
                                                        &mut self.tree_pos.y,
                                                        0.0..=512.0,
                                                    )
                                                    .text("Y"),
                                                )
                                                .changed();
                                            tree_desc_changed |= ui
                                                .add(
                                                    egui::Slider::new(
                                                        &mut self.tree_pos.z,
                                                        0.0..=1024.0,
                                                    )
                                                    .text("Z"),
                                                )
                                                .changed();

                                            ui.separator();
                                            tree_desc_changed |= self.tree_desc.edit_by_gui(ui);
                                        });

                                        ui.collapsing("Temporal Settings", |ui| {
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.temporal_position_phi,
                                                    0.0..=1.0,
                                                )
                                                .text("Position Phi"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.temporal_alpha,
                                                    0.0..=1.0,
                                                )
                                                .text("Alpha"),
                                            );
                                        });

                                        ui.collapsing("Spatial Settings", |ui| {
                                            ui.add(
                                                egui::Slider::new(&mut self.phi_c, 0.0..=1.0)
                                                    .text("Phi C"),
                                            );
                                            ui.add(
                                                egui::Slider::new(&mut self.phi_n, 0.0..=1.0)
                                                    .text("Phi N"),
                                            );
                                            ui.add(
                                                egui::Slider::new(&mut self.phi_p, 0.0..=1.0)
                                                    .text("Phi P"),
                                            );
                                            ui.add(
                                                egui::Slider::new(&mut self.min_phi_z, 0.0..=1.0)
                                                    .text("Min Phi Z"),
                                            );
                                            ui.add(
                                                egui::Slider::new(&mut self.max_phi_z, 0.0..=1.0)
                                                    .text("Max Phi Z"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.phi_z_stable_sample_count,
                                                    0.0..=1.0,
                                                )
                                                .text("Phi Z Stable Sample Count"),
                                            );
                                            ui.add(egui::Checkbox::new(
                                                &mut self.is_changing_lum_phi,
                                                "Changing Luminance Phi",
                                            ));
                                            ui.add(egui::Checkbox::new(
                                                &mut self.is_spatial_denoising_skipped,
                                                "Skip Spatial Denoising",
                                            ));
                                        });

                                        ui.collapsing("Anti-Aliasing", |ui| {
                                            ui.add(egui::Checkbox::new(
                                                &mut self.is_taa_enabled,
                                                "Enable Temporal Anti-Aliasing",
                                            ));
                                        });
                                    });
                                });
                        }

                        // FPS counter in bottom right
                        egui::Area::new("fps_counter".into())
                            .anchor(egui::Align2::RIGHT_BOTTOM, egui::Vec2::new(-10.0, -10.0))
                            .show(ctx, |ui| {
                                let fps_frame = egui::containers::Frame {
                                    fill: Color32::from_rgba_premultiplied(0, 0, 0, 128),
                                    inner_margin: egui::Margin::same(6),
                                    corner_radius: egui::CornerRadius::same(4),
                                    ..Default::default()
                                };

                                fps_frame.show(ui, |ui| {
                                    ui.allocate_ui_with_layout(
                                        egui::Vec2::new(80.0, 20.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{:.1}",
                                                    self.time_info.display_fps()
                                                ))
                                                .color(Color32::LIGHT_GRAY),
                                            );
                                        },
                                    );
                                });
                            });
                    });

                if tree_desc_changed {
                    self.add_a_tree(
                        self.tree_desc.clone(),
                        self.tree_pos,
                        true, // clean up before adding a new tree
                    )
                    .unwrap();
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
                    .update_buffers(
                        &self.time_info,
                        self.debug_float,
                        self.debug_bool,
                        self.debug_uint,
                        get_sun_dir(
                            self.sun_altitude.asin().to_degrees(),
                            self.sun_azimuth * 360.0,
                        ),
                        self.sun_size,
                        Vec3::new(
                            self.sun_color.r() as f32 / 255.0,
                            self.sun_color.g() as f32 / 255.0,
                            self.sun_color.b() as f32 / 255.0,
                        ),
                        self.sun_luminance,
                        self.sun_altitude,
                        self.sun_azimuth,
                        Vec3::new(
                            self.ambient_light.r() as f32 / 255.0,
                            self.ambient_light.g() as f32 / 255.0,
                            self.ambient_light.b() as f32 / 255.0,
                        ),
                        self.temporal_position_phi,
                        self.temporal_alpha,
                        self.phi_c,
                        self.phi_n,
                        self.phi_p,
                        self.min_phi_z,
                        self.max_phi_z,
                        self.phi_z_stable_sample_count,
                        self.is_changing_lum_phi,
                        self.is_spatial_denoising_skipped,
                        self.is_taa_enabled,
                        self.starlight_iterations,
                        self.starlight_formuparam,
                        self.starlight_volsteps,
                        self.starlight_stepsize,
                        self.starlight_zoom,
                        self.starlight_tile,
                        self.starlight_speed,
                        self.starlight_brightness,
                        self.starlight_darkmatter,
                        self.starlight_distfading,
                        self.starlight_saturation,
                    )
                    .unwrap();

                self.tracer
                    .record_trace(cmdbuf, &self.surface_builder.get_resources())
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

                self.vulkan_ctx
                    .wait_for_fences(&[self.fence.as_raw()])
                    .unwrap();

                self.tracer
                    .update_camera(frame_delta_time, self.is_fly_mode);
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
        self.tracer.on_resize(
            window_extent,
            &self.contree_builder.get_resources(),
            &self.scene_accel_builder.get_resources(),
        );

        // the render pass should be rebuilt when the swapchain is recreated
        self.egui_renderer
            .set_render_pass(self.swapchain.get_render_pass());

        self.is_resize_pending = false;
    }
}
