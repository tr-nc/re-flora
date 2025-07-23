#[allow(unused)]
use crate::util::Timer;

use crate::audio::{AudioEngine, ClipCache, PlayMode, SoundDataConfig};
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
use anyhow::Result;
use ash::vk;
use egui::{Color32, RichText};
use glam::{UVec3, Vec2, Vec3};
use gpu_allocator::vulkan::AllocatorCreateDesc;
use kira::{StartTime, Tween};
use rand::Rng;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::event::DeviceEvent;
use winit::{
    event::{ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
    window::WindowId,
};

#[derive(Debug, Clone)]
pub struct TreeVariationConfig {
    pub size_variance: f32,
    pub trunk_thickness_variance: f32,
    pub trunk_thickness_min_variance: f32,
    pub spread_variance: f32,
    pub randomness_variance: f32,
    pub vertical_tendency_variance: f32,
    pub branch_probability_variance: f32,
    pub leaves_size_level_variance: f32,
    pub iterations_variance: f32,
    pub tree_height_variance: f32,
    pub length_dropoff_variance: f32,
    pub thickness_reduction_variance: f32,
}

impl Default for TreeVariationConfig {
    fn default() -> Self {
        TreeVariationConfig {
            size_variance: 0.0,
            trunk_thickness_variance: 0.0,
            trunk_thickness_min_variance: 0.0,
            spread_variance: 0.0,
            randomness_variance: 0.0,
            vertical_tendency_variance: 0.0,
            branch_probability_variance: 0.0,
            leaves_size_level_variance: 0.0,
            iterations_variance: 0.0,
            tree_height_variance: 0.0,
            length_dropoff_variance: 0.0,
            thickness_reduction_variance: 0.0,
        }
    }
}

impl TreeVariationConfig {
    pub fn edit_by_gui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        ui.heading("Variation Settings");

        changed |= ui
            .add(egui::Slider::new(&mut self.size_variance, 0.0..=1.0).text("Size Variance"))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.trunk_thickness_variance, 0.0..=1.0)
                    .text("Thickness Variance"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.trunk_thickness_min_variance, 0.0..=1.0)
                    .text("Min Thickness Variance"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.iterations_variance, 0.0..=5.0)
                    .text("Iterations Variance"),
            )
            .changed();

        ui.separator();
        ui.heading("Shape Variation");

        changed |= ui
            .add(
                egui::Slider::new(&mut self.tree_height_variance, 0.0..=1.0)
                    .text("Height Variance"),
            )
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.spread_variance, 0.0..=1.0).text("Spread Variance"))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.vertical_tendency_variance, 0.0..=1.0)
                    .text("Vertical Tendency Variance"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.length_dropoff_variance, 0.0..=1.0)
                    .text("Length Dropoff Variance"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.thickness_reduction_variance, 0.0..=1.0)
                    .text("Thickness Reduction Variance"),
            )
            .changed();

        ui.separator();
        ui.heading("Branching Variation");

        changed |= ui
            .add(
                egui::Slider::new(&mut self.branch_probability_variance, 0.0..=1.0)
                    .text("Branch Probability Variance"),
            )
            .changed();

        ui.separator();
        ui.heading("Detail Variation");

        changed |= ui
            .add(
                egui::Slider::new(&mut self.randomness_variance, 0.0..=1.0)
                    .text("Randomness Variance"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.leaves_size_level_variance, 0.0..=5.0)
                    .text("Leaves Size Variance"),
            )
            .changed();

        changed
    }
}

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
    leaves_density_min: f32,
    leaves_density_max: f32,
    leaves_radius: f32,
    sun_altitude: f32,
    sun_azimuth: f32,
    sun_size: f32,
    auto_daynight_cycle: bool,
    time_of_day: f32,
    latitude: f32,
    season: f32,
    day_cycle_minutes: f32,
    sun_color: egui::Color32,
    sun_luminance: f32,
    ambient_light: egui::Color32,
    temporal_position_phi: f32,
    temporal_alpha: f32,
    god_ray_max_depth: f32,
    god_ray_max_checks: u32,
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
    tree_variation_config: TreeVariationConfig,
    regenerate_trees_requested: bool,
    prev_bound: UAabb3,

    // multi-tree management
    next_tree_id: u32,
    single_tree_id: u32, // ID for GUI single tree mode

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

    // grass colors
    grass_bottom_color: egui::Color32,
    grass_tip_color: egui::Color32,

    // leaf colors
    leaf_bottom_color: egui::Color32,
    leaf_tip_color: egui::Color32,

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
            leaves_density_min: 0.25,
            leaves_density_max: 0.5,
            leaves_radius: 15.0,
            temporal_position_phi: 0.8,
            temporal_alpha: 0.04,
            god_ray_max_depth: 3.0,
            god_ray_max_checks: 32,
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
            auto_daynight_cycle: true,
            time_of_day: 0.5,
            latitude: 0.5,
            season: 0.25,
            day_cycle_minutes: 30.0,
            sun_color: egui::Color32::from_rgb(255, 233, 144),
            sun_luminance: 1.0,
            ambient_light: egui::Color32::from_rgb(25, 25, 25),
            tree_pos: Vec3::new(512.0, 0.0, 512.0),
            tree_desc: TreeDesc::default(),
            tree_variation_config: TreeVariationConfig::default(),
            regenerate_trees_requested: false,
            prev_bound: Default::default(),
            config_panel_visible: false,
            is_fly_mode: true,

            // multi-tree management
            next_tree_id: 1, // Start from 1, use 0 for GUI single tree
            single_tree_id: 0,

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

            // Default grass colors (converted from resources.rs values)
            grass_bottom_color: egui::Color32::from_rgb(52, 116, 51),
            grass_tip_color: egui::Color32::from_rgb(182, 245, 0),

            // Default leaf colors (dark green to bright green)
            leaf_bottom_color: egui::Color32::from_rgb(143, 25, 153),
            leaf_tip_color: egui::Color32::from_rgb(255, 156, 224),

            audio_engine,
        };

        app.add_tree(app.tree_desc.clone(), app.tree_pos, true)?;

        // Configure leaves with the app's actual density values (now that app struct exists)
        app.tracer.regenerate_leaves(
            app.leaves_density_min,
            app.leaves_density_max,
            app.leaves_radius,
        )?;

        Ok(app)
    }

    fn generate_procedural_trees(&mut self) -> Result<()> {
        // Clear all procedural trees (keep single tree with ID 0)
        self.clear_procedural_trees()?;

        self.plain_builder.chunk_init(
            self.prev_bound.min(),
            self.prev_bound.max() - self.prev_bound.min(),
        )?;

        let world_size = CHUNK_DIM * VOXEL_DIM_PER_CHUNK;
        let map_padding = 50.0;
        let map_dimensions = Vec2::new(
            world_size.x as f32 - map_padding * 2.0,
            world_size.z as f32 - map_padding * 2.0,
        );
        let grid_size = 120.0;
        let mut placer_desc = PlacerDesc::new(42);
        placer_desc.threshold = 0.5;

        let tree_positions = generate_positions(
            map_dimensions,
            Vec2::new(map_padding, map_padding),
            grid_size,
            &placer_desc,
        );

        log::info!("Generated {} procedural trees", tree_positions.len());

        // Batch query all terrain heights at once
        let tree_positions_3d = self.query_terrain_heights_for_positions(&tree_positions)?;

        let mut rng = rand::rng();

        // Plant all trees with known heights and unique IDs
        for tree_pos in tree_positions_3d.iter() {
            let mut tree_desc = self.tree_desc.clone();
            tree_desc.seed = rng.random_range(1..10000);

            self.apply_tree_variations(&mut tree_desc, &mut rng);
            // Use add_procedural_tree_at_position for multi-tree support
            // No cleanup needed since we already cleaned up at the beginning
            self.add_procedural_tree_at_position(tree_desc, *tree_pos)?;
        }

        Ok(())
    }

    fn clear_procedural_trees(&mut self) -> Result<()> {
        // Remove all procedural tree leaves (IDs >= 1), keep single tree (ID 0)
        let tree_ids_to_remove: Vec<u32> = self
            .surface_builder
            .resources
            .instances
            .leaves_instances
            .keys()
            .filter(|&&id| id >= 1)
            .cloned()
            .collect();

        for tree_id in tree_ids_to_remove {
            self.tracer
                .remove_tree_leaves(&mut self.surface_builder.resources, tree_id)?;
        }

        log::info!("Cleared all procedural trees");
        Ok(())
    }

    fn add_procedural_tree_at_position(
        &mut self,
        tree_desc: TreeDesc,
        adjusted_tree_pos: Vec3,
    ) -> Result<()> {
        let tree_id = self.next_tree_id;
        self.next_tree_id += 1; // Increment for next tree

        let tree = Tree::new(tree_desc);
        let mut round_cones = Vec::new();
        for tree_trunk in tree.trunks() {
            let mut round_cone = tree_trunk.clone();
            round_cone.transform(adjusted_tree_pos);
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

        self.plain_builder.chunk_modify(&bvh_nodes, &round_cones)?;

        let relative_leaf_positions = tree.relative_leaf_positions();
        let offseted_leaf_positions = relative_leaf_positions
            .iter()
            .map(|leaf| *leaf + adjusted_tree_pos)
            .collect::<Vec<_>>();

        fn quantize(pos: &[Vec3]) -> Vec<UVec3> {
            let set = pos
                .iter()
                .map(|leaf| leaf.as_uvec3())
                .collect::<HashSet<_>>();
            return set.into_iter().collect::<Vec<_>>();
        }

        let quantized_leaf_positions = quantize(&offseted_leaf_positions);
        self.tracer.add_tree_leaves(
            &mut self.surface_builder.resources,
            tree_id,
            &quantized_leaf_positions,
        )?;

        Self::mesh_generate(
            &mut self.surface_builder,
            &mut self.contree_builder,
            &mut self.scene_accel_builder,
            this_bound.union_with(&self.prev_bound),
        )?;

        self.prev_bound = this_bound.union_with(&self.prev_bound);

        log::info!(
            "Added procedural tree {} at {:?}",
            tree_id,
            adjusted_tree_pos
        );
        Ok(())
    }

    fn edit_tree_with_variance(
        tree_desc: &mut TreeDesc,
        tree_variation_config: &mut TreeVariationConfig,
        ui: &mut egui::Ui,
    ) -> (bool, bool) {
        let mut regenerate_pressed = false;

        if ui.button("🌲 Regenerate Procedural Trees").clicked() {
            regenerate_pressed = true;
        }

        ui.separator();

        let tree_changed = tree_desc.edit_by_gui(ui);

        ui.separator();

        tree_variation_config.edit_by_gui(ui);

        (tree_changed, regenerate_pressed)
    }

    fn calculate_sun_position(&mut self, time_of_day: f32, latitude: f32, season: f32) {
        use std::f32::consts::PI;

        // Time of day: 0.0 = midnight, 0.5 = noon, 1.0 = midnight
        // Latitude: -1.0 = south pole, 0.0 = equator, 1.0 = north pole
        // Season: 0.0 = winter solstice, 0.25 = spring equinox, 0.5 = summer solstice, 0.75 = autumn equinox

        // Convert time to hour angle (radians)
        // Solar noon is at time_of_day = 0.5
        let hour_angle = (time_of_day - 0.5) * 2.0 * PI;

        // Solar declination based on season
        // Season of 0.0 = winter solstice (max negative declination)
        // Season of 0.5 = summer solstice (max positive declination)
        let seasonal_angle = season * 2.0 * PI;
        let declination = -23.44_f32.to_radians() * (seasonal_angle).cos(); // Earth's axial tilt

        // Calculate solar elevation (altitude)
        let elevation = (declination.sin() * (latitude * PI * 0.5).sin()
            + declination.cos() * (latitude * PI * 0.5).cos() * hour_angle.cos())
        .asin();

        // Calculate solar azimuth
        let azimuth = if hour_angle.cos() == 0.0 {
            if hour_angle > 0.0 {
                PI
            } else {
                0.0
            }
        } else {
            (declination.sin() * (latitude * PI * 0.5).cos()
                - declination.cos() * (latitude * PI * 0.5).sin() * hour_angle.cos())
            .atan2(hour_angle.sin())
        };

        // Normalize elevation to -1.0 to 1.0 range (matching current altitude range)
        self.sun_altitude = (elevation / (PI * 0.5)).clamp(-1.0, 1.0);

        // Normalize azimuth to 0.0 to 1.0 range (matching current azimuth range)
        self.sun_azimuth = ((azimuth + PI) / (2.0 * PI)) % 1.0;
    }

    fn apply_tree_variations(&self, tree_desc: &mut TreeDesc, rng: &mut impl Rng) {
        let config = &self.tree_variation_config;

        if config.size_variance > 0.0 {
            tree_desc.size *= 1.0 + rng.random_range(-config.size_variance..=config.size_variance);
        }

        if config.trunk_thickness_variance > 0.0 {
            tree_desc.trunk_thickness *= 1.0
                + rng.random_range(
                    -config.trunk_thickness_variance..=config.trunk_thickness_variance,
                );
        }

        if config.trunk_thickness_min_variance > 0.0 {
            tree_desc.trunk_thickness_min *= 1.0
                + rng.random_range(
                    -config.trunk_thickness_min_variance..=config.trunk_thickness_min_variance,
                );
        }

        if config.spread_variance > 0.0 {
            tree_desc.spread *=
                1.0 + rng.random_range(-config.spread_variance..=config.spread_variance);
        }

        if config.randomness_variance > 0.0 {
            tree_desc.randomness = (tree_desc.randomness
                + rng.random_range(-config.randomness_variance..=config.randomness_variance))
            .clamp(0.0, 1.0);
        }

        if config.vertical_tendency_variance > 0.0 {
            tree_desc.vertical_tendency = (tree_desc.vertical_tendency
                + rng.random_range(
                    -config.vertical_tendency_variance..=config.vertical_tendency_variance,
                ))
            .clamp(-1.0, 1.0);
        }

        if config.branch_probability_variance > 0.0 {
            tree_desc.branch_probability = (tree_desc.branch_probability
                + rng.random_range(
                    -config.branch_probability_variance..=config.branch_probability_variance,
                ))
            .clamp(0.0, 1.0);
        }

        if config.tree_height_variance > 0.0 {
            tree_desc.tree_height *=
                1.0 + rng.random_range(-config.tree_height_variance..=config.tree_height_variance);
        }

        if config.length_dropoff_variance > 0.0 {
            tree_desc.length_dropoff = (tree_desc.length_dropoff
                + rng.random_range(
                    -config.length_dropoff_variance..=config.length_dropoff_variance,
                ))
            .clamp(0.1, 1.0);
        }

        if config.thickness_reduction_variance > 0.0 {
            tree_desc.thickness_reduction = (tree_desc.thickness_reduction
                + rng.random_range(
                    -config.thickness_reduction_variance..=config.thickness_reduction_variance,
                ))
            .clamp(0.0, 1.0);
        }

        if config.iterations_variance > 0.0 {
            let variation =
                rng.random_range(-config.iterations_variance..=config.iterations_variance);
            tree_desc.iterations =
                ((tree_desc.iterations as f32 + variation).round() as u32).clamp(1, 12);
        }

        if config.leaves_size_level_variance > 0.0 {
            let variation = rng.random_range(
                -config.leaves_size_level_variance..=config.leaves_size_level_variance,
            );
            tree_desc.leaves_size_level =
                ((tree_desc.leaves_size_level as f32 + variation).round() as u32).clamp(0, 8);
        }
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

    fn query_terrain_heights_for_positions(&mut self, positions_2d: &[Vec2]) -> Result<Vec<Vec3>> {
        if positions_2d.is_empty() {
            return Ok(vec![]);
        }

        // Convert to query coordinates (divide by 256.0)
        let query_positions: Vec<Vec2> = positions_2d
            .iter()
            .map(|pos| Vec2::new(pos.x / 256.0, pos.y / 256.0))
            .collect();

        // Batch query all terrain heights
        let terrain_heights = self.tracer.query_terrain_heights_batch(&query_positions)?;

        // Convert back to world coordinates and create Vec3s
        let positions_3d = positions_2d
            .iter()
            .zip(terrain_heights.iter())
            .map(|(pos_2d, &height)| Vec3::new(pos_2d.x, height * 256.0, pos_2d.y))
            .collect();

        Ok(positions_3d)
    }

    fn add_tree(
        &mut self,
        tree_desc: TreeDesc,
        tree_pos: Vec3,
        clean_up_before_add: bool,
    ) -> Result<()> {
        // If we need to clean up first, do it before querying terrain to avoid
        // getting the wrong height due to existing tree geometry blocking the terrain query
        if clean_up_before_add {
            self.plain_builder.chunk_init(
                self.prev_bound.min(),
                self.prev_bound.max() - self.prev_bound.min(),
            )?;

            // Force mesh regeneration after cleanup to ensure terrain is properly accessible for querying
            Self::mesh_generate(
                &mut self.surface_builder,
                &mut self.contree_builder,
                &mut self.scene_accel_builder,
                self.prev_bound,
            )?;
        }

        let terrain_height = self
            .tracer
            .query_terrain_height(glam::Vec2::new(tree_pos.x / 256.0, tree_pos.z / 256.0))?;

        let terrain_height_scaled = terrain_height * 256.0;
        let adjusted_tree_pos = Vec3::new(tree_pos.x, terrain_height_scaled, tree_pos.z);

        self.add_tree_at_position(tree_desc, adjusted_tree_pos)
    }

    fn add_tree_at_position(&mut self, tree_desc: TreeDesc, adjusted_tree_pos: Vec3) -> Result<()> {
        let tree = Tree::new(tree_desc);
        let mut round_cones = Vec::new();
        for tree_trunk in tree.trunks() {
            let mut round_cone = tree_trunk.clone();
            round_cone.transform(adjusted_tree_pos);
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

        self.plain_builder.chunk_modify(&bvh_nodes, &round_cones)?;

        let relative_leaf_positions = tree.relative_leaf_positions();
        let offseted_leaf_positions = relative_leaf_positions
            .iter()
            .map(|leaf| *leaf + adjusted_tree_pos)
            .collect::<Vec<_>>();

        fn quantize(pos: &[Vec3]) -> Vec<UVec3> {
            let set = pos
                .iter()
                .map(|leaf| leaf.as_uvec3())
                .collect::<HashSet<_>>();
            return set.into_iter().collect::<Vec<_>>();
        }

        let quantized_leaf_positions = quantize(&offseted_leaf_positions);
        self.tracer.add_tree_leaves(
            &mut self.surface_builder.resources,
            self.single_tree_id,
            &quantized_leaf_positions,
        )?;

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

                                        ui.collapsing("Leaves Settings", |ui| {
                                            let mut leaves_changed = false;
                                            leaves_changed |= ui
                                                .add(
                                                    egui::Slider::new(
                                                        &mut self.leaves_density_min,
                                                        0.0..=1.0,
                                                    )
                                                    .text("Min Density"),
                                                )
                                                .changed();
                                            leaves_changed |= ui
                                                .add(
                                                    egui::Slider::new(
                                                        &mut self.leaves_density_max,
                                                        0.0..=1.0,
                                                    )
                                                    .text("Max Density"),
                                                )
                                                .changed();
                                            leaves_changed |= ui
                                                .add(
                                                    egui::Slider::new(
                                                        &mut self.leaves_radius,
                                                        1.0..=64.0,
                                                    )
                                                    .text("Radius"),
                                                )
                                                .changed();

                                            if leaves_changed {
                                                if let Err(e) = self.tracer.regenerate_leaves(
                                                    self.leaves_density_min,
                                                    self.leaves_density_max,
                                                    self.leaves_radius,
                                                ) {
                                                    log::error!(
                                                        "Failed to regenerate leaves: {}",
                                                        e
                                                    );
                                                }
                                            }
                                        });

                                        ui.collapsing("Sky Settings", |ui| {
                                            ui.add(egui::Checkbox::new(
                                                &mut self.auto_daynight_cycle,
                                                "Auto Day/Night Cycle",
                                            ));

                                            if self.auto_daynight_cycle {
                                                ui.add(
                                                    egui::Slider::new(
                                                        &mut self.time_of_day,
                                                        0.0..=1.0,
                                                    )
                                                    .text("Time of Day (0:00 - 23:59)")
                                                    .custom_formatter(|n, _| {
                                                        let hour = (n * 24.0) as u32 % 24;
                                                        let minute = (n * 24.0 * 60.0) as u32 % 60;
                                                        format!("{:02}:{:02}", hour, minute)
                                                    }),
                                                );

                                                ui.add(
                                                    egui::Slider::new(
                                                        &mut self.latitude,
                                                        -1.0..=1.0,
                                                    )
                                                    .text("Latitude (South Pole to North Pole)")
                                                    .custom_formatter(|n, _| {
                                                        if n < -0.5 {
                                                            format!("South ({:.1})", n)
                                                        } else if n > 0.5 {
                                                            format!("North ({:.1})", n)
                                                        } else {
                                                            format!("Equator ({:.1})", n)
                                                        }
                                                    }),
                                                );

                                                ui.add(
                                                    egui::Slider::new(&mut self.season, 0.0..=1.0)
                                                        .text("Season (Winter to Summer)")
                                                        .custom_formatter(|n, _| {
                                                            if n < 0.125 {
                                                                "Winter".to_string()
                                                            } else if n < 0.375 {
                                                                "Spring".to_string()
                                                            } else if n < 0.625 {
                                                                "Summer".to_string()
                                                            } else if n < 0.875 {
                                                                "Autumn".to_string()
                                                            } else {
                                                                "Winter".to_string()
                                                            }
                                                        }),
                                                );

                                                ui.add(
                                                    egui::Slider::new(
                                                        &mut self.day_cycle_minutes,
                                                        0.1..=60.0,
                                                    )
                                                    .text("Day Cycle Duration (Real Minutes)")
                                                    .custom_formatter(|n, _| {
                                                        if n < 1.0 {
                                                            format!("{:.1}s", n * 60.0)
                                                        } else {
                                                            format!("{:.1}m", n)
                                                        }
                                                    }),
                                                );

                                                // Read-only displays for calculated values
                                                ui.separator();
                                                ui.label(format!(
                                                    "Sun Altitude: {:.3}",
                                                    self.sun_altitude
                                                ));
                                                ui.label(format!(
                                                    "Sun Azimuth: {:.3}",
                                                    self.sun_azimuth
                                                ));
                                            } else {
                                                ui.add(
                                                    egui::Slider::new(
                                                        &mut self.sun_altitude,
                                                        -1.0..=1.0,
                                                    )
                                                    .text("Altitude (normalized)")
                                                    .smart_aim(false),
                                                );
                                                ui.add(
                                                    egui::Slider::new(
                                                        &mut self.sun_azimuth,
                                                        0.0..=1.0,
                                                    )
                                                    .text("Azimuth (normalized)"),
                                                );
                                            }
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
                                            let x_changed = ui
                                                .add(
                                                    egui::Slider::new(
                                                        &mut self.tree_pos.x,
                                                        0.0..=1024.0,
                                                    )
                                                    .text("X"),
                                                )
                                                .changed();
                                            tree_desc_changed |= x_changed;

                                            let z_changed = ui
                                                .add(
                                                    egui::Slider::new(
                                                        &mut self.tree_pos.z,
                                                        0.0..=1024.0,
                                                    )
                                                    .text("Z"),
                                                )
                                                .changed();
                                            tree_desc_changed |= z_changed;

                                            // Debug terrain height when X or Z position changes
                                            if x_changed || z_changed {
                                                // Clean up existing tree chunks before querying to avoid blocking the ray
                                                if let Err(e) = self.plain_builder.chunk_init(
                                                    self.prev_bound.min(),
                                                    self.prev_bound.max() - self.prev_bound.min(),
                                                ) {
                                                    log::error!("Failed to clean up chunks for terrain query: {}", e);
                                                } else {
                                                    // Force mesh regeneration after cleanup
                                                    if let Err(e) = Self::mesh_generate(
                                                        &mut self.surface_builder,
                                                        &mut self.contree_builder,
                                                        &mut self.scene_accel_builder,
                                                        self.prev_bound,
                                                    ) {
                                                        log::error!("Failed to regenerate mesh after cleanup: {}", e);
                                                    } else {
                                                        // Now query terrain height with clean terrain
                                                        match self.tracer.query_terrain_height(glam::Vec2::new(
                                                            self.tree_pos.x / 256.0,
                                                            self.tree_pos.z / 256.0,
                                                        )) {
                                                            Ok(terrain_height) => {
                                                                let terrain_height_scaled = terrain_height * 256.0;
                                                                log::info!("Debug terrain query - Position: ({}, {}), Terrain height: {}", 
                                                                    self.tree_pos.x, self.tree_pos.z, terrain_height_scaled);
                                                            }
                                                            Err(e) => {
                                                                log::error!("Failed to query terrain height: {}", e);
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            ui.separator();

                                            let (tree_changed, regenerate_pressed) =
                                                Self::edit_tree_with_variance(
                                                    &mut self.tree_desc,
                                                    &mut self.tree_variation_config,
                                                    ui,
                                                );
                                            tree_desc_changed |= tree_changed;

                                            if regenerate_pressed {
                                                self.regenerate_trees_requested = true;
                                            }
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

                                        ui.collapsing("God Ray Settings", |ui| {
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.god_ray_max_depth,
                                                    0.1..=10.0,
                                                )
                                                .text("Max Depth"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut self.god_ray_max_checks,
                                                    1..=64,
                                                )
                                                .text("Max Checks"),
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

                                        ui.collapsing("Grass Settings", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label("Bottom Color:");
                                                ui.color_edit_button_srgba(
                                                    &mut self.grass_bottom_color,
                                                );
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label("Tip Color:");
                                                ui.color_edit_button_srgba(
                                                    &mut self.grass_tip_color,
                                                );
                                            });
                                        });

                                        ui.collapsing("Leaf Settings", |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label("Bottom Color:");
                                                ui.color_edit_button_srgba(
                                                    &mut self.leaf_bottom_color,
                                                );
                                            });
                                            ui.horizontal(|ui| {
                                                ui.label("Tip Color:");
                                                ui.color_edit_button_srgba(
                                                    &mut self.leaf_tip_color,
                                                );
                                            });
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
                    self.add_tree(
                        self.tree_desc.clone(),
                        self.tree_pos,
                        true, // clean up before adding a new tree
                    )
                    .unwrap();
                }

                if self.regenerate_trees_requested {
                    self.regenerate_trees_requested = false;
                    match self.generate_procedural_trees() {
                        Ok(_) => {
                            log::info!("Procedural trees regenerated successfully");
                        }
                        Err(e) => {
                            log::error!("Failed to regenerate procedural trees: {}", e);
                        }
                    }
                }

                // Update sun position if auto day/night cycle is enabled
                if self.auto_daynight_cycle {
                    // Update time of day based on delta time and day cycle speed
                    // day_cycle_minutes is the real-world minutes for a full day cycle
                    // Convert to time progression per second: 1.0 / (day_cycle_minutes * 60.0)
                    let time_speed = 1.0 / (self.day_cycle_minutes * 60.0);
                    self.time_of_day += frame_delta_time * time_speed;

                    // Keep time_of_day in 0.0 to 1.0 range (wrap around)
                    self.time_of_day = self.time_of_day % 1.0;

                    self.calculate_sun_position(self.time_of_day, self.latitude, self.season);
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
                        self.god_ray_max_depth,
                        self.god_ray_max_checks,
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
                        Vec3::new(
                            self.grass_bottom_color.r() as f32 / 255.0,
                            self.grass_bottom_color.g() as f32 / 255.0,
                            self.grass_bottom_color.b() as f32 / 255.0,
                        ),
                        Vec3::new(
                            self.grass_tip_color.r() as f32 / 255.0,
                            self.grass_tip_color.g() as f32 / 255.0,
                            self.grass_tip_color.b() as f32 / 255.0,
                        ),
                        Vec3::new(
                            self.leaf_bottom_color.r() as f32 / 255.0,
                            self.leaf_bottom_color.g() as f32 / 255.0,
                            self.leaf_bottom_color.b() as f32 / 255.0,
                        ),
                        Vec3::new(
                            self.leaf_tip_color.r() as f32 / 255.0,
                            self.leaf_tip_color.g() as f32 / 255.0,
                            self.leaf_tip_color.b() as f32 / 255.0,
                        ),
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
