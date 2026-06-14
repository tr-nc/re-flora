mod resources;
pub use resources::*;

mod butterfly_palette;
pub use butterfly_palette::*;

mod palette_remap;

mod particle_texture_layout;
pub use particle_texture_layout::*;

mod denoiser_resources;
pub use denoiser_resources::*;

mod extent_dependent_resources;
pub use extent_dependent_resources::*;

mod vertex;
pub use vertex::*;

pub mod voxel_encoding;

mod voxel_geometry;

mod leaves_construct;

mod pipeline_builder;
use pipeline_builder::*;

mod buffer_updater;
use buffer_updater::*;

use glam::{Mat4, UVec3, Vec2, Vec3};
use winit::event::KeyEvent;

const LEAF_INSTANCE_TYPE: u32 = 4;
const APPLE_INSTANCE_TYPE: u32 = 5;
const APPLE_BOTTOM_COLOR: Vec3 = Vec3::new(0.48, 0.025, 0.018);
const APPLE_TIP_COLOR: Vec3 = Vec3::new(0.95, 0.06, 0.035);

use crate::audio::SpatialSoundManager;
use crate::builder::{
    ContreeBuilderResources, FloraInstanceResources, SceneAccelBuilderResources, SurfaceResources,
    TreeLeavesInstance,
};
use crate::gameplay::{
    calculate_directional_light_matrices, Camera, CameraDesc, CameraPose, CameraVectors,
};
use crate::generated::gpu_structs::{PushConstantFlora, PushConstantLeafShadowTemporal};
use crate::geom::UAabb3;
use crate::particles::{ParticleSnapshot, PARTICLE_CAPACITY};
use crate::resource::ResourceContainer;
use crate::util::{ShaderCompiler, TimeInfo};
use crate::wind::WindSource;
use anyhow::Result;
use std::collections::HashMap;
use verdarium_vkn::vk;
use verdarium_vkn::{
    execute_one_time_gpu_job, Allocator, ClearValue, ColorClearValue, CommandBuffer,
    ComputePipeline, DepthOrStencilClearValue, DescriptorPool, Extent2D, Extent3D, Framebuffer,
    GpuProfiler, GraphicsPipeline, MemoryAccess, MemoryBarrier, PipelineBarrier, PipelineStage,
    PushConstantInfo, RenderPass, RenderTarget, Texture, TextureLayout, Viewport, VulkanContext,
};

const MAX_TERRAIN_QUERIES: usize = 1_000;
const SHADOW_MAP_RESOLUTION: u32 = 1024;
const CLOUD_SHADOW_MAP_RESOLUTION: u32 = 256;
const LEAF_SHADOW_OPACITY_RESOLUTION: u32 = 2048;
pub(super) const WIND_VOLUME_BUCKET_COUNT: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WindVolumePushConstants {
    time: f32,
    bucket_index: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GlassPushConstants {
    box_min_near_alpha: [f32; 4],
    box_max_far_alpha: [f32; 4],
}

const TERRARIUM_GLASS_NEAR_ALPHA: f32 = 0.045;
const TERRARIUM_GLASS_FAR_ALPHA: f32 = 0.12;

#[derive(Debug, Clone)]
pub struct WindGuiParams {
    pub sources: Vec<WindSource>,
}

#[derive(Debug, Clone, Copy)]
pub struct CloudGuiParams {
    pub enabled: bool,
    pub coverage: f32,
    pub density: f32,
    pub bottom_height: f32,
    pub top_height: f32,
    pub shape_scale: f32,
    pub detail_scale: f32,
    pub detail_strength: f32,
    pub wind_speed: f32,
    pub primary_steps: u32,
    pub light_steps: u32,
    pub temporal_alpha: f32,
    pub absorption: f32,
    pub phase_eccentricity: f32,
    pub silver_intensity: f32,
    pub max_distance: f32,
    pub shadows_enabled: bool,
    pub shadow_debug_overlay: bool,
    pub shadow_strength: f32,
    pub shadow_min_transmittance: f32,
    pub shadow_steps: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct WindSourceGpu {
    pub params: [f32; 4],
    pub noise: [f32; 4],
}

impl From<WindSource> for WindSourceGpu {
    fn from(source: WindSource) -> Self {
        Self {
            params: [source.direction_degrees, source.speed, source.gain, 0.0],
            noise: [
                source.pattern_scale,
                source.octaves as f32,
                source.lacunarity,
                source.persistence,
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VsmFilterPushConstants {
    blur_radius: u32,
    temporal_alpha: f32,
    reset_history: u32,
    _pad0: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CloudTemporalPushConstants {
    reset_history: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CloudShadowTemporalPushConstants {
    reset_history: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct TerrainRayQuery {
    pub origin: Vec3,
    pub direction: Vec3,
}

fn should_render_grass_species(species_index: usize, grass_render_mode: u32) -> bool {
    match species_index {
        0 => grass_render_mode != 2,
        1 => grass_render_mode != 1,
        _ => true,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TerrainRayHitSample {
    pub position: Vec3,
    pub is_valid: bool,
}

fn flora_push_constant(
    time: f32,
    instance_ty: u32,
    chunk_world_offset: UVec3,
    bottom_color: Vec3,
    tip_color: Vec3,
) -> PushConstantFlora {
    PushConstantFlora {
        time,
        instance_ty,
        chunk_world_offset: chunk_world_offset.to_array(),
        bottom_color: bottom_color.to_array(),
        tip_color: tip_color.to_array(),
        ..bytemuck::Zeroable::zeroed()
    }
}

pub struct TracerDesc {
    pub scaling_factor: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LodState {
    Lod0,
    Lod1,
}

pub struct Tracer {
    vulkan_ctx: VulkanContext,

    desc: TracerDesc,
    chunk_bound: UAabb3,

    allocator: Allocator,
    resources: TracerResources,
    particle_resources: ParticleRendererResources,

    camera: Camera,
    camera_view_mat_prev_frame: Mat4,
    camera_proj_mat_prev_frame: Mat4,
    current_view_proj_mat: Mat4,
    shadow_camera_initialized: bool,
    shadow_map_history_valid: bool,
    leaf_shadow_history_valid: bool,
    cloud_history_valid: bool,
    cloud_shadow_history_valid: bool,

    compute_pipelines: ComputePipelines,
    graphics_pipelines: GraphicsPipelines,

    render_target_color_and_depth: RenderTarget,
    render_target_depth_only: RenderTarget,
    render_target_leaf_shadow_opacity: RenderTarget,

    #[allow(dead_code)]
    pool: DescriptorPool,

    a_trous_iteration_count: u32,
    world_tick_seconds: f32,
    last_wind_volume_step: Option<u32>,
    initialized_wind_volume_bucket_count: u32,
    wind_source_buffer_capacity: usize,
    spatial_sound_manager: SpatialSoundManager,
    particle_instance_scratch: Vec<ParticleInstanceGpu>,
}

impl Drop for Tracer {
    fn drop(&mut self) {}
}

impl Tracer {
    fn default_camera_pose_for_bound(chunk_bound: UAabb3) -> (Vec3, f32, f32) {
        let min = chunk_bound.min().as_vec3();
        let max = chunk_bound.max().as_vec3();
        let extent = max - min;
        let center = (min + max) * 0.5;
        let horizontal_extent = extent.x.max(extent.z).max(1.0);
        let camera_distance = horizontal_extent * 0.7 + 0.65;
        let camera_height = extent.y.max(1.0) * 0.28;
        let camera_position = center + Vec3::new(0.0, camera_height, camera_distance);
        let look_direction = (center - camera_position).normalize();
        let yaw_deg = look_direction.x.atan2(-look_direction.z).to_degrees();
        let horizontal_len = Vec2::new(look_direction.x, look_direction.z).length();
        let pitch_deg = look_direction.y.atan2(horizontal_len).to_degrees();
        (camera_position, yaw_deg, pitch_deg)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        chunk_bound: UAabb3,
        screen_extent: Extent2D,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
        desc: TracerDesc,
        spatial_sound_manager: SpatialSoundManager,
    ) -> Result<Self> {
        let render_extent = Self::get_render_extent(screen_extent, desc.scaling_factor);
        let (camera_position, camera_yaw_deg, camera_pitch_deg) =
            Self::default_camera_pose_for_bound(chunk_bound);

        let camera = Camera::new(
            camera_position,
            camera_yaw_deg,
            camera_pitch_deg,
            CameraDesc {
                aspect_ratio: render_extent.get_aspect_ratio(),
                ..Default::default()
            },
            spatial_sound_manager.clone(),
        )?;

        let pool = DescriptorPool::new(vulkan_ctx.device()).unwrap();

        let shader_modules = PipelineBuilder::create_shader_modules(&vulkan_ctx, shader_compiler)?;

        let resources = TracerResources::new(
            &vulkan_ctx,
            allocator.clone(),
            &shader_modules.tracer_sm,
            &shader_modules.tracer_shadow_sm,
            &shader_modules.composition_sm,
            &shader_modules.temporal_sm,
            &shader_modules.spatial_sm,
            &shader_modules.god_ray_sm,
            &shader_modules.post_processing_sm,
            &shader_modules.player_collider_sm,
            &shader_modules.terrain_query_sm,
            &shader_modules.flora_vert_sm,
            chunk_bound,
            render_extent,
            screen_extent,
            Extent2D::new(SHADOW_MAP_RESOLUTION, SHADOW_MAP_RESOLUTION),
            Extent2D::new(CLOUD_SHADOW_MAP_RESOLUTION, CLOUD_SHADOW_MAP_RESOLUTION),
            Extent2D::new(
                LEAF_SHADOW_OPACITY_RESOLUTION,
                LEAF_SHADOW_OPACITY_RESOLUTION,
            ),
            MAX_TERRAIN_QUERIES as u32,
        );
        let particle_resources =
            ParticleRendererResources::new(vulkan_ctx.device().clone(), allocator.clone());

        let compute_pipelines = PipelineBuilder::create_compute_pipelines(
            &vulkan_ctx,
            &shader_modules,
            &pool,
            &resources,
            contree_builder_resources,
            scene_accel_resources,
        );
        let render_passes = PipelineBuilder::create_render_passes(
            &vulkan_ctx,
            resources.extent_dependent_resources.gfx_output_tex.clone(),
            resources.extent_dependent_resources.gfx_depth_tex.clone(),
            resources.shadow.shadow_map_depth_tex.clone(),
            resources.shadow.leaf_shadow_opacity_tex.clone(),
        );

        let graphics_pipelines = PipelineBuilder::create_graphics_pipelines(
            &vulkan_ctx,
            &shader_modules,
            &render_passes,
            &pool,
            &resources,
        );

        let framebuffer_color_and_depth = Self::create_framebuffer_color_and_depth(
            &vulkan_ctx,
            &render_passes.render_pass_color_and_depth,
            &resources.extent_dependent_resources.gfx_output_tex,
            &resources.extent_dependent_resources.gfx_depth_tex,
        );
        let framebuffer_depth_only = Self::create_framebuffer_depth(
            &vulkan_ctx,
            &render_passes.render_pass_depth,
            &resources.shadow.shadow_map_depth_tex,
        );
        let framebuffer_leaf_shadow_opacity = Self::create_framebuffer_color(
            &vulkan_ctx,
            &render_passes.render_pass_leaf_shadow_opacity,
            &resources.shadow.leaf_shadow_opacity_tex,
        );

        let render_target_color_and_depth = RenderTarget::new(
            render_passes.render_pass_color_and_depth,
            vec![framebuffer_color_and_depth],
        );
        let render_target_depth_only = RenderTarget::new(
            render_passes.render_pass_depth,
            vec![framebuffer_depth_only],
        );
        let render_target_leaf_shadow_opacity = RenderTarget::new(
            render_passes.render_pass_leaf_shadow_opacity,
            vec![framebuffer_leaf_shadow_opacity],
        );

        let particle_capacity = PARTICLE_CAPACITY;

        Ok(Self {
            vulkan_ctx,
            desc,
            chunk_bound,
            allocator,
            resources,
            particle_resources,
            camera,
            camera_view_mat_prev_frame: Mat4::IDENTITY,
            camera_proj_mat_prev_frame: Mat4::IDENTITY,
            current_view_proj_mat: Mat4::IDENTITY,
            shadow_camera_initialized: false,
            shadow_map_history_valid: false,
            leaf_shadow_history_valid: false,
            cloud_history_valid: false,
            cloud_shadow_history_valid: false,
            compute_pipelines,
            graphics_pipelines,
            render_target_color_and_depth,
            render_target_depth_only,
            render_target_leaf_shadow_opacity,
            pool,
            a_trous_iteration_count: 3,
            world_tick_seconds: crate::game_time::WORLD_TICK_SECONDS_DEFAULT,
            last_wind_volume_step: None,
            initialized_wind_volume_bucket_count: 0,
            wind_source_buffer_capacity: 1,
            spatial_sound_manager,
            particle_instance_scratch: Vec::with_capacity(particle_capacity),
        })
    }

    /// A framebuffer that contains the color and depth textures for the main render pass
    fn create_framebuffer_color_and_depth(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        target_texture: &Texture,
        depth_texture: &Texture,
    ) -> Framebuffer {
        let target_image_extent = target_texture
            .get_image()
            .get_desc()
            .extent
            .as_extent_2d()
            .unwrap();

        Framebuffer::from_textures(
            vulkan_ctx.clone(),
            render_pass,
            &[target_texture, depth_texture],
            target_image_extent,
        )
        .unwrap()
    }

    /// A framebuffer that contains a color texture.
    fn create_framebuffer_color(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        color_tex: &Texture,
    ) -> Framebuffer {
        let color_extent = color_tex
            .get_image()
            .get_desc()
            .extent
            .as_extent_2d()
            .unwrap();
        Framebuffer::from_textures(vulkan_ctx.clone(), render_pass, &[color_tex], color_extent)
            .unwrap()
    }

    /// A framebuffer that contains the shadow map texture
    fn create_framebuffer_depth(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        shadow_map_tex: &Texture,
    ) -> Framebuffer {
        let shadow_image_extent = shadow_map_tex
            .get_image()
            .get_desc()
            .extent
            .as_extent_2d()
            .unwrap();
        Framebuffer::from_textures(
            vulkan_ctx.clone(),
            render_pass,
            &[shadow_map_tex],
            shadow_image_extent,
        )
        .unwrap()
    }

    pub fn on_resize(
        &mut self,
        screen_extent: Extent2D,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
    ) {
        let render_extent = Self::get_render_extent(screen_extent, self.desc.scaling_factor);

        self.camera.on_resize(render_extent);

        // this must be done first
        self.resources.on_resize(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            render_extent,
            screen_extent,
        );

        let framebuffer_color_and_depth = Self::create_framebuffer_color_and_depth(
            &self.vulkan_ctx,
            self.render_target_color_and_depth.get_render_pass(),
            &self.resources.extent_dependent_resources.gfx_output_tex,
            &self.resources.extent_dependent_resources.gfx_depth_tex,
        );
        let framebuffer_depth_only = Self::create_framebuffer_depth(
            &self.vulkan_ctx,
            self.render_target_depth_only.get_render_pass(),
            &self.resources.shadow.shadow_map_depth_tex,
        );
        let framebuffer_leaf_shadow_opacity = Self::create_framebuffer_color(
            &self.vulkan_ctx,
            self.render_target_leaf_shadow_opacity.get_render_pass(),
            &self.resources.shadow.leaf_shadow_opacity_tex,
        );

        self.render_target_color_and_depth = RenderTarget::new(
            self.render_target_color_and_depth.get_render_pass().clone(),
            vec![framebuffer_color_and_depth],
        );
        self.render_target_depth_only = RenderTarget::new(
            self.render_target_depth_only.get_render_pass().clone(),
            vec![framebuffer_depth_only],
        );
        self.render_target_leaf_shadow_opacity = RenderTarget::new(
            self.render_target_leaf_shadow_opacity
                .get_render_pass()
                .clone(),
            vec![framebuffer_leaf_shadow_opacity],
        );

        self.cloud_history_valid = false;
        self.cloud_shadow_history_valid = false;
        self.update_sets(contree_builder_resources, scene_accel_resources);
    }

    fn update_sets(
        &mut self,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
    ) {
        let update_compute_fn = |ppl: &ComputePipeline, resources: &[&dyn ResourceContainer]| {
            ppl.auto_update_descriptor_sets(resources).unwrap()
        };

        let update_graphics_fn = |ppl: &GraphicsPipeline, resources: &[&dyn ResourceContainer]| {
            ppl.auto_update_descriptor_sets(resources).unwrap()
        };

        let all_resources =
            self.all_descriptor_resources(contree_builder_resources, scene_accel_resources);
        update_compute_fn(&self.compute_pipelines.tracer_ppl, &all_resources);
        update_compute_fn(&self.compute_pipelines.tracer_shadow_ppl, &all_resources);
        update_compute_fn(&self.compute_pipelines.player_collider_ppl, &all_resources);
        update_compute_fn(&self.compute_pipelines.terrain_query_ppl, &all_resources);

        let tracer_resources = self.tracer_descriptor_resources();
        update_compute_fn(&self.compute_pipelines.wind_volume_ppl, &tracer_resources);
        update_compute_fn(
            &self.compute_pipelines.shadow_depth_copy_ppl,
            &tracer_resources,
        );
        update_compute_fn(
            &self.compute_pipelines.leaf_shadow_mask_ppl,
            &tracer_resources,
        );
        update_compute_fn(&self.compute_pipelines.vsm_creation_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.vsm_blur_h_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.vsm_blur_v_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.god_ray_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.temporal_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.spatial_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.cloud_ppl, &tracer_resources);
        update_compute_fn(&self.compute_pipelines.cloud_shadow_ppl, &tracer_resources);
        update_compute_fn(
            &self.compute_pipelines.cloud_shadow_temporal_ppl,
            &tracer_resources,
        );
        update_compute_fn(
            &self.compute_pipelines.cloud_temporal_ppl,
            &tracer_resources,
        );
        update_compute_fn(&self.compute_pipelines.lens_flare_ppl, &tracer_resources);
        update_compute_fn(
            &self.compute_pipelines.lens_flare_sun_visible_ppl,
            &tracer_resources,
        );
        update_compute_fn(
            &self.compute_pipelines.lens_flare_downsample_ppl,
            &tracer_resources,
        );
        update_compute_fn(&self.compute_pipelines.composition_ppl, &tracer_resources);
        update_compute_fn(
            &self.compute_pipelines.post_processing_ppl,
            &tracer_resources,
        );

        // update graphics pipelines descriptor sets
        update_graphics_fn(&self.graphics_pipelines.flora_ppl, &tracer_resources);
        update_graphics_fn(&self.graphics_pipelines.flora_lod_ppl, &tracer_resources);
        update_graphics_fn(&self.graphics_pipelines.leaves_ppl, &tracer_resources);
        update_graphics_fn(&self.graphics_pipelines.leaves_lod_ppl, &tracer_resources);
        update_graphics_fn(
            &self.graphics_pipelines.leaves_shadow_lod_ppl,
            &tracer_resources,
        );
        update_graphics_fn(&self.graphics_pipelines.particle_ppl, &tracer_resources);
    }

    fn tracer_descriptor_resources(&self) -> [&dyn ResourceContainer; 1] {
        [&self.resources as &dyn ResourceContainer]
    }

    fn all_descriptor_resources<'a>(
        &'a self,
        contree_builder_resources: &'a ContreeBuilderResources,
        scene_accel_resources: &'a SceneAccelBuilderResources,
    ) -> [&'a dyn ResourceContainer; 3] {
        [
            &self.resources as &dyn ResourceContainer,
            contree_builder_resources as &dyn ResourceContainer,
            scene_accel_resources as &dyn ResourceContainer,
        ]
    }

    // create a lower resolution texture for rendering, for better performance,
    // less memory usage, and stylized rendering
    fn get_render_extent(screen_extent: Extent2D, scaling_factor: f32) -> Extent2D {
        Extent2D::new(
            (screen_extent.width as f32 * scaling_factor) as u32,
            (screen_extent.height as f32 * scaling_factor) as u32,
        )
    }

    pub fn get_screen_output_tex(&self) -> &Texture {
        &self.resources.extent_dependent_resources.screen_output_tex
    }

    fn ensure_wind_source_buffer_capacity(&mut self, source_count: usize) -> Result<()> {
        let required_capacity = source_count.max(1);
        if required_capacity <= self.wind_source_buffer_capacity {
            return Ok(());
        }

        let new_capacity = required_capacity.next_power_of_two();
        *self.resources.wind.wind_sources = TracerResources::create_wind_sources_buffer(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            new_capacity,
        );
        self.wind_source_buffer_capacity = new_capacity;
        let tracer_resources = self.tracer_descriptor_resources();
        self.compute_pipelines
            .wind_volume_ppl
            .auto_update_descriptor_sets(&tracer_resources)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_buffers(
        &mut self,
        time_info: &TimeInfo,
        debug_float: f32,
        debug_bool: bool,
        debug_uint: u32,
        flora_instance_hsv_offset_max: Vec3,
        flora_voxel_hsv_offset_max: Vec3,
        grass_bottom_dark: Vec3,
        grass_bottom_light: Vec3,
        grass_tip_dark: Vec3,
        grass_tip_light: Vec3,
        world_tick_seconds: f32,
        update_shadow_map: bool,
        lens_flare_intensity: f32,
        lens_flare_sun_pixel_scale: f32,
        wind_directional_bias_fraction: f32,
        wind_turbulence_fraction: f32,
        grass_vibration_amplitude_voxels: f32,
        grass_vibration_primary_speed: f32,
        grass_vibration_secondary_speed: f32,
        grass_natural_bend_min_voxels: f32,
        grass_natural_bend_max_voxels: f32,
        flora_bend_height_power: f32,
        flora_player_push_radius: f32,
        flora_player_push_strength: f32,
        leaf_paddle_amplitude_voxels: f32,
        leaf_paddle_primary_speed: f32,
        leaf_paddle_secondary_speed: f32,
        leaf_paddle_amplitude_wind_start_strength: f32,
        leaf_paddle_amplitude_wind_full_strength: f32,
        leaf_paddle_amplitude_wind_knee_bias: f32,
        leaf_paddle_frequency_wind_start_strength: f32,
        leaf_paddle_frequency_wind_full_strength: f32,
        leaf_paddle_frequency_wind_knee_bias: f32,
        leaf_paddle_frequency_min_multiplier: f32,
        leaf_paddle_frequency_max_multiplier: f32,
        leaf_shadow_fragment_opacity: f32,
        leaf_shadow_strength: f32,
        leaf_shadow_min_transmittance: f32,
        leaf_shadow_filter_radius_texels: f32,
        wind_gui_params: WindGuiParams,
        cloud_gui_params: CloudGuiParams,
        flora_tick: u32,
        sprout_delay_ticks: u32,
        full_growth_ticks: u32,
        sun_dir: Vec3,
        sun_size: f32,
        sun_color: Vec3,
        sun_luminance: f32,
        sun_display_luminance: f32,
        sun_altitude: f32,
        sun_azimuth: f32,
        ambient_light: Vec3,
        temporal_position_phi: f32,
        temporal_alpha: f32,
        phi_c: f32,
        phi_n: f32,
        phi_p: f32,
        min_phi_z: f32,
        max_phi_z: f32,
        phi_z_stable_sample_count: f32,
        is_changing_lum_phi: bool,
        is_spatial_denoising_enabled: bool,
        a_trous_iteration_count: u32,
        god_ray_max_depth: f32,
        god_ray_max_checks: u32,
        god_ray_weight: f32,
        god_ray_color: Vec3,
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
        voxel_dirt_color: Vec3,
        voxel_sand_color: Vec3,
        voxel_cherry_wood_color: Vec3,
        voxel_oak_wood_color: Vec3,
        voxel_rock_color: Vec3,
        voxel_color_variance: f32,
    ) -> Result<()> {
        let view_mat = self.camera.get_view_mat();
        let proj_mat = self.camera.get_proj_mat();
        self.current_view_proj_mat = proj_mat * view_mat;
        BufferUpdater::update_camera_info(
            &mut self.resources.uniforms.camera_info,
            view_mat,
            proj_mat,
        )?;

        // Shadow camera info. Shadow maps are rendered every frame while shadows
        // are enabled, so PCSS and VSM both use the latest light-space matrix.
        if update_shadow_map || !self.shadow_camera_initialized {
            let world_bound = self.chunk_bound.into();
            let shadow_map_extent = self
                .resources
                .shadow
                .shadow_map_tex
                .get_image()
                .get_desc()
                .extent;
            let shadow_map_resolution = shadow_map_extent.width.min(shadow_map_extent.height);
            let (shadow_view_mat, shadow_proj_mat) =
                calculate_directional_light_matrices(world_bound, sun_dir, shadow_map_resolution);
            self.shadow_camera_initialized = true;
            BufferUpdater::update_camera_info(
                &mut self.resources.shadow.shadow_camera_info,
                shadow_view_mat,
                shadow_proj_mat,
            )?;
        }

        // camera info prev frame
        BufferUpdater::update_camera_info(
            &mut self.resources.uniforms.camera_info_prev_frame,
            self.camera_view_mat_prev_frame,
            self.camera_proj_mat_prev_frame,
        )?;

        BufferUpdater::update_god_ray_info(
            &self.resources,
            god_ray_max_depth,
            god_ray_max_checks,
            god_ray_weight,
            god_ray_color,
        )?;

        BufferUpdater::update_post_processing_info(&self.resources, self.desc.scaling_factor)?;

        BufferUpdater::update_voxel_colors(
            &self.resources,
            voxel_dirt_color,
            voxel_sand_color,
            voxel_cherry_wood_color,
            voxel_oak_wood_color,
            voxel_rock_color,
            voxel_color_variance,
        )?;

        self.world_tick_seconds = crate::game_time::clamp_world_tick_seconds(world_tick_seconds);

        self.ensure_wind_source_buffer_capacity(wind_gui_params.sources.len())?;
        BufferUpdater::update_gui_input(
            &self.resources,
            debug_float,
            debug_bool,
            debug_uint,
            flora_instance_hsv_offset_max,
            flora_voxel_hsv_offset_max,
            grass_bottom_dark,
            grass_bottom_light,
            grass_tip_dark,
            grass_tip_light,
            lens_flare_intensity,
            lens_flare_sun_pixel_scale,
            wind_directional_bias_fraction,
            wind_turbulence_fraction,
            self.world_tick_seconds,
            grass_vibration_amplitude_voxels,
            grass_vibration_primary_speed,
            grass_vibration_secondary_speed,
            grass_natural_bend_min_voxels,
            grass_natural_bend_max_voxels,
            flora_bend_height_power,
            flora_player_push_radius,
            flora_player_push_strength,
            leaf_paddle_amplitude_voxels,
            leaf_paddle_primary_speed,
            leaf_paddle_secondary_speed,
            leaf_paddle_amplitude_wind_start_strength,
            leaf_paddle_amplitude_wind_full_strength,
            leaf_paddle_amplitude_wind_knee_bias,
            leaf_paddle_frequency_wind_start_strength,
            leaf_paddle_frequency_wind_full_strength,
            leaf_paddle_frequency_wind_knee_bias,
            leaf_paddle_frequency_min_multiplier,
            leaf_paddle_frequency_max_multiplier,
            leaf_shadow_fragment_opacity,
            leaf_shadow_strength,
            leaf_shadow_min_transmittance,
            leaf_shadow_filter_radius_texels,
            wind_gui_params,
            cloud_gui_params,
        )?;

        BufferUpdater::update_flora_growth_info(
            &self.resources,
            flora_tick,
            sprout_delay_ticks,
            full_growth_ticks,
        )?;

        BufferUpdater::update_sun_info(
            &self.resources,
            sun_dir,
            sun_size,
            sun_color,
            sun_luminance,
            sun_display_luminance,
            sun_altitude,
            sun_azimuth,
        )?;

        BufferUpdater::update_shading_info(&self.resources, ambient_light)?;

        BufferUpdater::update_starlight_info(
            &self.resources,
            starlight_iterations,
            starlight_formuparam,
            starlight_volsteps,
            starlight_stepsize,
            starlight_zoom,
            starlight_tile,
            starlight_speed,
            starlight_brightness,
            starlight_darkmatter,
            starlight_distfading,
            starlight_saturation,
        )?;

        BufferUpdater::update_env_info(&self.resources, time_info.total_frame_count() as u32)?;

        BufferUpdater::update_denoiser_info(
            &mut self.resources.denoiser_resources.temporal_info,
            &mut self.resources.denoiser_resources.spatial_info,
            temporal_position_phi,
            temporal_alpha,
            phi_c,
            phi_n,
            phi_p,
            min_phi_z,
            max_phi_z,
            phi_z_stable_sample_count,
            is_changing_lum_phi,
            is_spatial_denoising_enabled,
        )?;

        // Update the a_trous_iteration_count field
        self.a_trous_iteration_count = a_trous_iteration_count;

        self.camera_view_mat_prev_frame = self.camera.get_view_mat();
        self.camera_proj_mat_prev_frame = self.camera.get_proj_mat();

        Ok(())
    }

    /// Returns a list of chunks that need to be drawn this frame.
    fn chunks_needs_to_draw_this_frame<'a>(
        &self,
        surface_resources: &'a SurfaceResources,
        lod_distance: f32,
        flora_draw_distance: f32,
    ) -> HashMap<LodState, Vec<&'a FloraInstanceResources>> {
        let mut lod0_instances = Vec::new();
        let mut lod1_instances = Vec::new();
        let camera_pos = self.camera.position();

        for (aabb, instances) in &surface_resources.instances.chunk_flora_instances {
            // perform frustum culling
            if !aabb.is_inside_frustum(self.current_view_proj_mat) {
                continue;
            }

            // calculate distance from camera to chunk center
            let chunk_center = aabb.center();
            let distance = (camera_pos - chunk_center).length();

            // skip chunks beyond max flora draw distance
            if distance > flora_draw_distance {
                continue;
            }

            if distance <= lod_distance {
                lod0_instances.push(instances);
            } else {
                lod1_instances.push(instances);
            }
        }

        let mut result = HashMap::new();
        result.insert(LodState::Lod0, lod0_instances);
        result.insert(LodState::Lod1, lod1_instances);
        result
    }

    fn trees_needs_to_draw_this_frame<'a>(
        &self,
        tree_instances: &'a HashMap<u32, TreeLeavesInstance>,
        lod_distance: f32,
        flora_draw_distance: f32,
    ) -> HashMap<LodState, Vec<&'a TreeLeavesInstance>> {
        let mut lod0_instances = Vec::new();
        let mut lod1_instances = Vec::new();
        let camera_pos = self.camera.position();

        for tree_instance in tree_instances.values() {
            // perform frustum culling
            if !tree_instance
                .aabb
                .is_inside_frustum(self.current_view_proj_mat)
            {
                continue;
            }

            // calculate distance from camera to tree center
            let tree_center = tree_instance.aabb.center();
            let distance = (camera_pos - tree_center).length();

            if distance > flora_draw_distance {
                continue;
            }

            if distance <= lod_distance {
                lod0_instances.push(tree_instance);
            } else {
                lod1_instances.push(tree_instance);
            }
        }

        let mut result = HashMap::new();
        result.insert(LodState::Lod0, lod0_instances);
        result.insert(LodState::Lod1, lod1_instances);
        result
    }

    fn with_gpu_scope<T>(
        gpu_profiler: Option<&mut GpuProfiler>,
        gpu_profiler_frame_slot: usize,
        cmdbuf: &CommandBuffer,
        name: &'static str,
        work: impl FnOnce() -> T,
    ) -> T {
        let Some(profiler) = gpu_profiler else {
            return work();
        };
        let scope = profiler.begin_scope(
            gpu_profiler_frame_slot,
            cmdbuf,
            name,
            PipelineStage::ALL_COMMANDS,
        );
        let result = work();
        if let Some(scope) = scope {
            profiler.end_scope(
                gpu_profiler_frame_slot,
                cmdbuf,
                scope,
                PipelineStage::ALL_COMMANDS,
            );
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_trace(
        &mut self,
        cmdbuf: &CommandBuffer,
        surface_resources: &SurfaceResources,
        lod_distance: f32,
        flora_draw_distance: f32,
        grass_render_mode: u32,
        time: f32,
        flora_colors: &[(Vec3, Vec3)],
        leaf_bottom_color: Vec3,
        leaf_tip_color: Vec3,
        render_flags: &crate::RenderFlags,
        update_shadow_map: bool,
        vsm_blur_radius: u32,
        vsm_temporal_alpha: f32,
        leaf_shadow_temporal_alpha: f32,
        reset_vsm_history: bool,
        mut gpu_profiler: Option<&mut GpuProfiler>,
        gpu_profiler_frame_slot: usize,
    ) -> Result<()> {
        self.graphics_pipelines
            .begin_manual_buffer_frame(gpu_profiler_frame_slot);

        let compute_to_compute_barrier = PipelineBarrier::compute_shader_access();
        // VSM filtering writes shadow_map_tex_for_vsm_ping in compute, then the
        // flora vertex shader samples it in the same command buffer. MoltenVK/Metal
        // needs the write made visible to graphics explicitly; a compute->compute
        // barrier is not enough and causes close grass shadow flicker on macOS.
        let compute_to_graphics_barrier = PipelineBarrier::shader_access(
            PipelineStage::COMPUTE_SHADER,
            PipelineStage::VERTEX_SHADER,
        );
        let frag_to_vert_barrier = PipelineBarrier::shader_access(
            PipelineStage::FRAGMENT_SHADER,
            PipelineStage::VERTEX_SHADER,
        );
        let color_to_compute_barrier = PipelineBarrier::new(
            PipelineStage::COLOR_ATTACHMENT_OUTPUT,
            PipelineStage::COMPUTE_SHADER,
            [MemoryBarrier::new(
                MemoryAccess::COLOR_ATTACHMENT_WRITE,
                MemoryAccess::SHADER_READ | MemoryAccess::SHADER_WRITE,
            )],
        );
        let compute_to_transfer_barrier = PipelineBarrier::new(
            PipelineStage::COMPUTE_SHADER,
            PipelineStage::TRANSFER,
            [MemoryBarrier::new(
                MemoryAccess::SHADER_WRITE,
                MemoryAccess::TRANSFER_READ | MemoryAccess::TRANSFER_WRITE,
            )],
        );
        let transfer_to_compute_barrier = PipelineBarrier::new(
            PipelineStage::TRANSFER,
            PipelineStage::COMPUTE_SHADER,
            [MemoryBarrier::new(
                MemoryAccess::TRANSFER_READ | MemoryAccess::TRANSFER_WRITE,
                MemoryAccess::SHADER_READ | MemoryAccess::SHADER_WRITE,
            )],
        );

        Self::with_gpu_scope(
            gpu_profiler.as_deref_mut(),
            gpu_profiler_frame_slot,
            cmdbuf,
            "clear.targets",
            || self.record_clear_render_targets(cmdbuf, render_flags, update_shadow_map),
        );

        let enable_glass = render_flags.enable_tracer;
        let has_graphics_pass =
            render_flags.enable_flora || render_flags.enable_particles || enable_glass;

        if render_flags.enable_flora {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "wind_volume.pass",
                || self.record_wind_volume_pass(cmdbuf, time),
            );

            let b1 = PipelineBarrier::shader_access(
                PipelineStage::COMPUTE_SHADER,
                PipelineStage::VERTEX_SHADER | PipelineStage::COMPUTE_SHADER,
            );
            b1.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        if render_flags.enable_flora && render_flags.enable_shadows && update_shadow_map {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "leaf_shadow_opacity.pass",
                || {
                    self.record_leaves_shadow_lod_pass(
                        cmdbuf,
                        surface_resources,
                        leaf_bottom_color,
                        leaf_tip_color,
                        time,
                    )
                },
            );
            color_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "leaf_shadow_temporal.pass",
                || {
                    self.record_leaf_shadow_temporal_pass(
                        cmdbuf,
                        leaf_shadow_temporal_alpha,
                        reset_vsm_history,
                    )
                },
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "leaf_shadow_mask.pass",
                || self.record_leaf_shadow_mask_pass(cmdbuf),
            );
            compute_to_transfer_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            self.record_store_leaf_shadow_history(cmdbuf);
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }
        if has_graphics_pass || (render_flags.enable_shadows && update_shadow_map) {
            let frag_to_compute_barrier = PipelineBarrier::shader_access(
                PipelineStage::FRAGMENT_SHADER,
                PipelineStage::COMPUTE_SHADER,
            );
            frag_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        if render_flags.enable_shadows && update_shadow_map {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "shadow_depth_copy.pass",
                || self.record_shadow_depth_copy_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "tracer_shadow.pass",
                || self.record_tracer_shadow_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "vsm_filtering.pass",
                || {
                    self.record_vsm_filtering_pass(
                        cmdbuf,
                        vsm_blur_radius,
                        vsm_temporal_alpha,
                        reset_vsm_history,
                    )
                },
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            if render_flags.enable_clouds {
                Self::with_gpu_scope(
                    gpu_profiler.as_deref_mut(),
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "cloud_shadow.pass",
                    || self.record_cloud_shadow_pass(cmdbuf),
                );
                compute_to_transfer_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
                self.record_store_cloud_shadow_history(cmdbuf);
                transfer_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            } else {
                self.cloud_shadow_history_valid = false;
            }
            compute_to_graphics_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        if has_graphics_pass && !render_flags.enable_flora {
            let b1 = PipelineBarrier::shader_access(
                PipelineStage::COMPUTE_SHADER,
                PipelineStage::VERTEX_SHADER | PipelineStage::COMPUTE_SHADER,
            );
            b1.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        if render_flags.enable_flora {
            assert_eq!(
                flora_colors.len(),
                self.resources.meshes.flora_meshes.len(),
                "Flora color count ({}) must match flora mesh count ({})",
                flora_colors.len(),
                self.resources.meshes.flora_meshes.len()
            );
        }
        if has_graphics_pass {
            if let Some(profiler) = gpu_profiler.as_deref_mut() {
                let graphics_scope = profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.pass",
                    PipelineStage::ALL_COMMANDS,
                );
                self.record_all_graphics_passes(
                    cmdbuf,
                    surface_resources,
                    lod_distance,
                    flora_draw_distance,
                    grass_render_mode,
                    flora_colors,
                    leaf_bottom_color,
                    leaf_tip_color,
                    time,
                    render_flags.enable_flora,
                    render_flags.enable_particles,
                    enable_glass,
                    Some(profiler),
                    gpu_profiler_frame_slot,
                );
                if let Some(scope) = graphics_scope {
                    profiler.end_scope(
                        gpu_profiler_frame_slot,
                        cmdbuf,
                        scope,
                        PipelineStage::ALL_COMMANDS,
                    );
                }
            } else {
                self.record_all_graphics_passes(
                    cmdbuf,
                    surface_resources,
                    lod_distance,
                    flora_draw_distance,
                    grass_render_mode,
                    flora_colors,
                    leaf_bottom_color,
                    leaf_tip_color,
                    time,
                    render_flags.enable_flora,
                    render_flags.enable_particles,
                    enable_glass,
                    None,
                    gpu_profiler_frame_slot,
                );
            }
            frag_to_vert_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        if render_flags.enable_tracer {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "tracer.pass",
                || self.record_tracer_pass(cmdbuf),
            );
        } else {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "tracer_clear.pass",
                || self.clear_tracer_outputs(cmdbuf),
            );
        }

        if has_graphics_pass || render_flags.enable_tracer {
            let b2 = PipelineBarrier::shader_access(
                PipelineStage::FRAGMENT_SHADER | PipelineStage::COMPUTE_SHADER,
                PipelineStage::COMPUTE_SHADER,
            );
            b2.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        if render_flags.enable_god_rays {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "god_ray.pass",
                || self.record_god_ray_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }

        if render_flags.enable_denoiser {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "denoiser.pass",
                || self.record_denoiser_pass(cmdbuf, self.a_trous_iteration_count),
            )?;
        }

        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        if render_flags.enable_clouds {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "cloud.pass",
                || self.record_cloud_pass(cmdbuf),
            );
            compute_to_transfer_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            self.record_store_cloud_history(cmdbuf);
            transfer_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        } else {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "cloud_clear.pass",
                || self.clear_cloud_output(cmdbuf),
            );
        }
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        if render_flags.enable_lens_flare {
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "lens_flare_sun_visible.pass",
                || self.record_lens_flare_sun_visible_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "lens_flare.pass",
                || self.record_lens_flare_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            Self::with_gpu_scope(
                gpu_profiler.as_deref_mut(),
                gpu_profiler_frame_slot,
                cmdbuf,
                "lens_flare_downsample.pass",
                || self.record_lens_flare_downsample_pass(cmdbuf),
            );
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        }
        Self::with_gpu_scope(
            gpu_profiler.as_deref_mut(),
            gpu_profiler_frame_slot,
            cmdbuf,
            "composition.pass",
            || self.record_composition_pass(cmdbuf),
        );
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        if let Some(profiler) = gpu_profiler {
            let postprocessing_scope = profiler.begin_scope(
                gpu_profiler_frame_slot,
                cmdbuf,
                "post_processing.pass",
                PipelineStage::ALL_COMMANDS,
            );
            self.record_post_processing_pass(cmdbuf);
            if let Some(scope) = postprocessing_scope {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        } else {
            self.record_post_processing_pass(cmdbuf);
        }
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        if render_flags.enable_denoiser {
            copy_current_to_prev(&self.resources, cmdbuf);
        }

        return Ok(());

        fn copy_current_to_prev(resources: &TracerResources, cmdbuf: &CommandBuffer) {
            let copy_fn = |src_tex: &Texture, dst_tex: &Texture| {
                src_tex.get_image().record_copy_to(
                    cmdbuf,
                    dst_tex.get_image(),
                    TextureLayout::GENERAL,
                    TextureLayout::GENERAL,
                );
            };

            for history in [
                resources.denoiser_resources.tex.temporal.normal_history(),
                resources.denoiser_resources.tex.temporal.position_history(),
                resources.denoiser_resources.tex.temporal.vox_id_history(),
                resources.denoiser_resources.tex.temporal.accumed_history(),
            ] {
                copy_fn(history.current(), history.previous());
            }
        }
    }

    fn record_store_vsm_history(&self, cmdbuf: &CommandBuffer) {
        let history = self.resources.shadow.vsm_history();
        history.current().get_image().record_copy_to(
            cmdbuf,
            history.previous().get_image(),
            TextureLayout::GENERAL,
            TextureLayout::GENERAL,
        );
    }

    fn record_store_cloud_history(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .cloud_output_tex
            .get_image()
            .record_copy_to(
                cmdbuf,
                self.resources
                    .extent_dependent_resources
                    .cloud_history_tex
                    .get_image(),
                TextureLayout::GENERAL,
                TextureLayout::GENERAL,
            );
    }

    fn record_store_cloud_shadow_history(&self, cmdbuf: &CommandBuffer) {
        let history = self.resources.shadow.cloud_shadow_history();
        history.current().get_image().record_copy_to(
            cmdbuf,
            history.previous().get_image(),
            TextureLayout::GENERAL,
            TextureLayout::GENERAL,
        );
    }

    fn record_clear_render_targets(
        &self,
        cmdbuf: &CommandBuffer,
        render_flags: &crate::RenderFlags,
        update_shadow_map: bool,
    ) {
        let has_graphics_pass = render_flags.enable_flora || render_flags.enable_particles;
        if !has_graphics_pass {
            self.resources
                .extent_dependent_resources
                .gfx_output_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
                );
            self.resources
                .extent_dependent_resources
                .gfx_depth_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::DepthStencil(DepthOrStencilClearValue::Depth(1.0)),
                );
        }

        if update_shadow_map {
            self.resources
                .shadow
                .shadow_map_depth_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::DepthStencil(DepthOrStencilClearValue::Depth(1.0)),
                );

            self.resources
                .shadow
                .shadow_map_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::Float([1.0, 0.0, 0.0, 0.0])),
                );

            for tex in [
                &self.resources.shadow.cloud_shadow_raw_tex,
                &self.resources.shadow.cloud_shadow_tex,
            ] {
                tex.get_image().record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::Float([1.0, 0.0, 0.0, 0.0])),
                );
            }

            self.resources
                .shadow
                .leaf_shadow_opacity_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
                );

            self.resources
                .shadow
                .leaf_shadow_mask_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
                );
        }

        self.resources
            .extent_dependent_resources
            .god_ray_output_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
            );

        self.resources
            .denoiser_resources
            .tex
            .spatial_ping_pong()
            .pong()
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
            );

        self.resources
            .extent_dependent_resources
            .lens_flare_full_output_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
            );

        self.resources
            .extent_dependent_resources
            .lens_flare_output_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
            );

        if render_flags.enable_lens_flare {
            self.resources
                .extent_dependent_resources
                .lens_flare_visible_count_tex
                .get_image()
                .record_clear(
                    cmdbuf,
                    Some(TextureLayout::GENERAL),
                    0,
                    ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
                );
        }
    }

    /// Draw all flora species (LOD0+LOD1), leaves (LOD0+LOD1), and particles
    /// inside a single Vulkan render pass to avoid tile-memory load/store
    /// overhead on tile-based GPUs (Apple Silicon via MoltenVK) and prevent
    /// the particle pass from clearing the flora output.
    #[allow(clippy::too_many_arguments)]
    fn record_all_graphics_passes(
        &self,
        cmdbuf: &CommandBuffer,
        surface_resources: &SurfaceResources,
        lod_distance: f32,
        flora_draw_distance: f32,
        grass_render_mode: u32,
        flora_colors: &[(Vec3, Vec3)],
        leaf_bottom_color: Vec3,
        leaf_tip_color: Vec3,
        time: f32,
        enable_flora: bool,
        enable_particles: bool,
        enable_glass: bool,
        mut gpu_profiler: Option<&mut GpuProfiler>,
        gpu_profiler_frame_slot: usize,
    ) {
        let render_target = &self.render_target_color_and_depth;

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.0],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        if enable_flora {
            for pipeline in [
                &self.graphics_pipelines.flora_ppl,
                &self.graphics_pipelines.flora_lod_ppl,
                &self.graphics_pipelines.leaves_ppl,
                &self.graphics_pipelines.leaves_lod_ppl,
            ] {
                pipeline.record_texture_transitions(cmdbuf);
            }
        }
        if enable_particles {
            self.graphics_pipelines
                .particle_ppl
                .record_texture_transitions(cmdbuf);
        }
        if enable_glass {
            self.graphics_pipelines
                .glass_ppl
                .record_texture_transitions(cmdbuf);
        }

        Self::with_gpu_scope(
            gpu_profiler.as_deref_mut(),
            gpu_profiler_frame_slot,
            cmdbuf,
            "graphics.renderpass.begin",
            || render_target.record_begin(cmdbuf, &clear_values),
        );

        let render_extent = self
            .resources
            .extent_dependent_resources
            .gfx_output_tex
            .get_image()
            .get_desc()
            .extent;
        let viewport = Viewport::from_extent(render_extent.as_extent_2d().unwrap());
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: render_extent.width,
                height: render_extent.height,
            },
        };

        // Draw all flora species, both LOD levels
        if enable_flora {
            let flora_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.flora",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let chunks_by_lod = self.chunks_needs_to_draw_this_frame(
                surface_resources,
                lod_distance,
                flora_draw_distance,
            );
            for (species_index, (bottom_color, tip_color)) in flora_colors.iter().enumerate() {
                if !should_render_grass_species(species_index, grass_render_mode) {
                    continue;
                }

                for &lod_state in &[LodState::Lod0, LodState::Lod1] {
                    let pipeline = match lod_state {
                        LodState::Lod0 => &self.graphics_pipelines.flora_ppl,
                        LodState::Lod1 => &self.graphics_pipelines.flora_lod_ppl,
                    };
                    let mesh_collection = match lod_state {
                        LodState::Lod0 => &self.resources.meshes.flora_meshes,
                        LodState::Lod1 => &self.resources.meshes.flora_meshes_lod,
                    };
                    let mesh = mesh_collection.get(species_index).unwrap_or_else(|| {
                        panic!("Missing flora mesh for species index {}", species_index)
                    });

                    pipeline.record_bind(cmdbuf);
                    pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);

                    cmdbuf.bind_index_buffer_u32(&mesh.indices);

                    let flora_instances = &chunks_by_lod[&lod_state];
                    for instances in flora_instances.iter() {
                        let instance_count = instances.species_len(species_index);
                        if instance_count == 0 {
                            continue;
                        }
                        let instance_offset = FloraInstanceResources::species_offset(species_index);
                        let push_constant = flora_push_constant(
                            time,
                            species_index as u32,
                            instances.chunk_world_offset,
                            *bottom_color,
                            *tip_color,
                        );

                        cmdbuf.bind_vertex_buffers(0, &[&mesh.vertices]);
                        pipeline.record_indexed_with_manual_buffer(
                            cmdbuf,
                            1,
                            0,
                            &instances.resource.instances_buf,
                            mesh.indices_len,
                            instance_count,
                            0,
                            0,
                            instance_offset,
                            Some(&PushConstantInfo {
                                shader_stage: vk::ShaderStageFlags::VERTEX,
                                push_constants: bytemuck::bytes_of(&push_constant).to_vec(),
                            }),
                        );
                    }
                }
            }
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), flora_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }

            // Draw leaves, both LOD levels
            let leaves_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.leaves",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let trees_by_lod = self.trees_needs_to_draw_this_frame(
                &surface_resources.instances.leaves_instances,
                lod_distance,
                flora_draw_distance,
            );
            for &lod_state in &[LodState::Lod0, LodState::Lod1] {
                let pipeline = match lod_state {
                    LodState::Lod0 => &self.graphics_pipelines.leaves_ppl,
                    LodState::Lod1 => &self.graphics_pipelines.leaves_lod_ppl,
                };
                let (indices_buf, vertices_buf, indices_len) = match lod_state {
                    LodState::Lod0 => (
                        &self.resources.meshes.leaves_resources.indices,
                        &self.resources.meshes.leaves_resources.vertices,
                        self.resources.meshes.leaves_resources.indices_len,
                    ),
                    LodState::Lod1 => (
                        &self.resources.meshes.leaves_resources_lod.indices,
                        &self.resources.meshes.leaves_resources_lod.vertices,
                        self.resources.meshes.leaves_resources_lod.indices_len,
                    ),
                };

                let leaves_instances = &trees_by_lod[&lod_state];
                if leaves_instances.is_empty() {
                    continue;
                }

                pipeline.record_bind(cmdbuf);
                pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);

                cmdbuf.bind_index_buffer_u32(indices_buf);

                for tree_instance in leaves_instances.iter() {
                    if tree_instance.resources.instances_len == 0 {
                        continue;
                    }
                    let leaf_push = flora_push_constant(
                        time,
                        LEAF_INSTANCE_TYPE,
                        tree_instance.chunk_world_offset,
                        leaf_bottom_color,
                        leaf_tip_color,
                    );
                    cmdbuf.bind_vertex_buffers(0, &[vertices_buf]);
                    pipeline.record_indexed_with_manual_buffer(
                        cmdbuf,
                        1,
                        0,
                        &tree_instance.resources.instances_buf,
                        indices_len,
                        tree_instance.resources.instances_len,
                        0,
                        0,
                        0,
                        Some(&PushConstantInfo {
                            shader_stage: vk::ShaderStageFlags::VERTEX,
                            push_constants: bytemuck::bytes_of(&leaf_push).to_vec(),
                        }),
                    );
                }
            }
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), leaves_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }

            // Draw apples as render-only tree fruit instances using the same
            // foliage shaders as leaves, but with a smaller apple mesh and a
            // distinct instance type for color/motion.
            let apples_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.apples",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let apples_by_lod = self.trees_needs_to_draw_this_frame(
                &surface_resources.instances.apple_instances,
                lod_distance,
                flora_draw_distance,
            );
            for &lod_state in &[LodState::Lod0, LodState::Lod1] {
                let pipeline = match lod_state {
                    LodState::Lod0 => &self.graphics_pipelines.leaves_ppl,
                    LodState::Lod1 => &self.graphics_pipelines.leaves_lod_ppl,
                };
                let (indices_buf, vertices_buf, indices_len) = match lod_state {
                    LodState::Lod0 => (
                        &self.resources.meshes.apple_resources.indices,
                        &self.resources.meshes.apple_resources.vertices,
                        self.resources.meshes.apple_resources.indices_len,
                    ),
                    LodState::Lod1 => (
                        &self.resources.meshes.apple_resources_lod.indices,
                        &self.resources.meshes.apple_resources_lod.vertices,
                        self.resources.meshes.apple_resources_lod.indices_len,
                    ),
                };

                let apple_instances = &apples_by_lod[&lod_state];
                if apple_instances.is_empty() {
                    continue;
                }

                pipeline.record_bind(cmdbuf);
                pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);
                cmdbuf.bind_index_buffer_u32(indices_buf);

                for tree_instance in apple_instances.iter() {
                    if tree_instance.resources.instances_len == 0 {
                        continue;
                    }
                    let apple_push = flora_push_constant(
                        time,
                        APPLE_INSTANCE_TYPE,
                        tree_instance.chunk_world_offset,
                        APPLE_BOTTOM_COLOR,
                        APPLE_TIP_COLOR,
                    );
                    cmdbuf.bind_vertex_buffers(0, &[vertices_buf]);
                    pipeline.record_indexed_with_manual_buffer(
                        cmdbuf,
                        1,
                        0,
                        &tree_instance.resources.instances_buf,
                        indices_len,
                        tree_instance.resources.instances_len,
                        0,
                        0,
                        0,
                        Some(&PushConstantInfo {
                            shader_stage: vk::ShaderStageFlags::VERTEX,
                            push_constants: bytemuck::bytes_of(&apple_push).to_vec(),
                        }),
                    );
                }
            }
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), apples_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        } // end enable_flora

        // Draw particles in the same render pass (no second CLEAR)
        if enable_particles {
            let particles_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.particles",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let particle_resources = &self.particle_resources;
            if particle_resources.instance_count > 0 {
                let particle_ppl = &self.graphics_pipelines.particle_ppl;
                particle_ppl.record_bind(cmdbuf);
                particle_ppl.record_viewport_scissor(cmdbuf, viewport, scissor);

                cmdbuf.bind_index_buffer_u32(&particle_resources.indices);
                cmdbuf.bind_vertex_buffers(
                    0,
                    &[
                        &particle_resources.vertices,
                        &particle_resources.instance_buffer,
                    ],
                );

                particle_ppl.record_indexed(
                    cmdbuf,
                    particle_resources.indices_len,
                    particle_resources.instance_count,
                    0,
                    0,
                    0,
                    None,
                );
            }
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), particles_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        }

        if enable_glass {
            let glass_scope = gpu_profiler.as_deref_mut().and_then(|profiler| {
                profiler.begin_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    "graphics.terrarium_glass",
                    PipelineStage::ALL_COMMANDS,
                )
            });
            let glass = &self.resources.meshes.glass;
            if glass.indices_len > 0 {
                let glass_ppl = &self.graphics_pipelines.glass_ppl;
                glass_ppl.record_bind(cmdbuf);
                glass_ppl.record_viewport_scissor(cmdbuf, viewport, scissor);
                cmdbuf.bind_index_buffer_u32(&glass.indices);
                cmdbuf.bind_vertex_buffers(0, &[&glass.vertices]);

                let box_min = glass.box_min;
                let box_max = glass.box_max;
                let push = GlassPushConstants {
                    box_min_near_alpha: [
                        box_min.x,
                        box_min.y,
                        box_min.z,
                        TERRARIUM_GLASS_NEAR_ALPHA,
                    ],
                    box_max_far_alpha: [box_max.x, box_max.y, box_max.z, TERRARIUM_GLASS_FAR_ALPHA],
                };
                let box_center = (box_min + box_max) * 0.5;
                let face_centers = [
                    Vec3::new(box_min.x, box_center.y, box_center.z),
                    Vec3::new(box_max.x, box_center.y, box_center.z),
                    Vec3::new(box_center.x, box_center.y, box_min.z),
                    Vec3::new(box_center.x, box_center.y, box_max.z),
                ];
                let camera_position = self.camera.position();
                let mut face_order = [0usize, 1, 2, 3];
                face_order.sort_by(|left, right| {
                    let left_dist = face_centers[*left].distance_squared(camera_position);
                    let right_dist = face_centers[*right].distance_squared(camera_position);
                    left_dist
                        .partial_cmp(&right_dist)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                // Draw the nearest glass faces first while depth writing is enabled. This keeps the
                // front pane as the single composited transparent layer over ray-traced terrain,
                // instead of accumulating all four walls and washing terrain out.
                for face_id in face_order {
                    glass_ppl.record_indexed(
                        cmdbuf,
                        6,
                        1,
                        face_id as u32 * 6,
                        0,
                        0,
                        Some(&PushConstantInfo {
                            shader_stage: vk::ShaderStageFlags::VERTEX,
                            push_constants: bytemuck::bytes_of(&push).to_vec(),
                        }),
                    );
                }
            }
            if let (Some(profiler), Some(scope)) = (gpu_profiler.as_deref_mut(), glass_scope) {
                profiler.end_scope(
                    gpu_profiler_frame_slot,
                    cmdbuf,
                    scope,
                    PipelineStage::ALL_COMMANDS,
                );
            }
        }

        Self::with_gpu_scope(
            gpu_profiler.as_deref_mut(),
            gpu_profiler_frame_slot,
            cmdbuf,
            "graphics.renderpass.end",
            || render_target.record_end(cmdbuf),
        );
    }

    fn record_leaves_shadow_lod_pass(
        &self,
        cmdbuf: &CommandBuffer,
        surface_resources: &SurfaceResources,
        bottom_color: Vec3,
        tip_color: Vec3,
        time: f32,
    ) {
        self.graphics_pipelines
            .leaves_shadow_lod_ppl
            .record_texture_transitions(cmdbuf);
        self.graphics_pipelines
            .leaves_shadow_lod_ppl
            .record_bind(cmdbuf);

        let clear_values: [vk::ClearValue; 0] = [];

        self.render_target_leaf_shadow_opacity
            .record_begin(cmdbuf, &clear_values);

        let shadow_extent = self
            .resources
            .shadow
            .leaf_shadow_opacity_tex
            .get_image()
            .get_desc()
            .extent;
        let viewport = Viewport::from_extent(shadow_extent.as_extent_2d().unwrap());
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: shadow_extent.width,
                height: shadow_extent.height,
            },
        };

        self.graphics_pipelines
            .leaves_shadow_lod_ppl
            .record_viewport_scissor(cmdbuf, viewport, scissor);

        cmdbuf.bind_index_buffer_u32(&self.resources.meshes.leaves_resources_lod.indices);

        // loop through all tree leaves instances
        for tree_instance in surface_resources.instances.leaves_instances.values() {
            if tree_instance.resources.instances_len == 0 {
                continue;
            }
            let push_constant = flora_push_constant(
                time,
                LEAF_INSTANCE_TYPE,
                tree_instance.chunk_world_offset,
                bottom_color,
                tip_color,
            );

            cmdbuf.bind_vertex_buffers(0, &[&self.resources.meshes.leaves_resources_lod.vertices]);
            // render this instance for shadow map
            self.graphics_pipelines
                .leaves_shadow_lod_ppl
                .record_indexed_with_manual_buffer(
                    cmdbuf,
                    1,
                    0,
                    &tree_instance.resources.instances_buf,
                    self.resources.meshes.leaves_resources_lod.indices_len,
                    tree_instance.resources.instances_len,
                    0,
                    0,
                    0,
                    Some(&PushConstantInfo {
                        shader_stage: vk::ShaderStageFlags::VERTEX,
                        push_constants: bytemuck::bytes_of(&push_constant).to_vec(),
                    }),
                );
        }

        cmdbuf.bind_index_buffer_u32(&self.resources.meshes.apple_resources_lod.indices);
        for tree_instance in surface_resources.instances.apple_instances.values() {
            if tree_instance.resources.instances_len == 0 {
                continue;
            }
            let push_constant = flora_push_constant(
                time,
                APPLE_INSTANCE_TYPE,
                tree_instance.chunk_world_offset,
                APPLE_BOTTOM_COLOR,
                APPLE_TIP_COLOR,
            );

            cmdbuf.bind_vertex_buffers(0, &[&self.resources.meshes.apple_resources_lod.vertices]);
            self.graphics_pipelines
                .leaves_shadow_lod_ppl
                .record_indexed_with_manual_buffer(
                    cmdbuf,
                    1,
                    0,
                    &tree_instance.resources.instances_buf,
                    self.resources.meshes.apple_resources_lod.indices_len,
                    tree_instance.resources.instances_len,
                    0,
                    0,
                    0,
                    Some(&PushConstantInfo {
                        shader_stage: vk::ShaderStageFlags::VERTEX,
                        push_constants: bytemuck::bytes_of(&push_constant).to_vec(),
                    }),
                );
        }

        self.render_target_leaf_shadow_opacity.record_end(cmdbuf);
    }

    fn record_tracer_shadow_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.tracer_shadow_ppl.record(
            cmdbuf,
            self.resources
                .shadow
                .shadow_map_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_shadow_depth_copy_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.shadow_depth_copy_ppl.record(
            cmdbuf,
            self.resources
                .shadow
                .shadow_map_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_leaf_shadow_temporal_pass(
        &mut self,
        cmdbuf: &CommandBuffer,
        temporal_alpha: f32,
        reset_leaf_shadow_history: bool,
    ) {
        let reset_history = reset_leaf_shadow_history || !self.leaf_shadow_history_valid;
        let push_constants = PushConstantLeafShadowTemporal {
            temporal_alpha: temporal_alpha.clamp(0.0, 1.0),
            reset_history: u32::from(reset_history),
            ..bytemuck::Zeroable::zeroed()
        };
        let push_constants_bytes = bytemuck::bytes_of(&push_constants);
        self.compute_pipelines.leaf_shadow_temporal_ppl.record(
            cmdbuf,
            self.resources
                .shadow
                .leaf_shadow_opacity_blended_tex
                .get_image()
                .get_desc()
                .extent,
            Some(push_constants_bytes),
        );
        self.leaf_shadow_history_valid = true;
    }

    fn record_leaf_shadow_mask_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.leaf_shadow_mask_ppl.record(
            cmdbuf,
            self.resources
                .shadow
                .leaf_shadow_mask_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_store_leaf_shadow_history(&self, cmdbuf: &CommandBuffer) {
        let history = self.resources.shadow.leaf_shadow_history();
        history.current().get_image().record_copy_to(
            cmdbuf,
            history.previous().get_image(),
            TextureLayout::GENERAL,
            TextureLayout::GENERAL,
        );
    }

    fn wind_volume_bucket_step_seconds(&self) -> f32 {
        self.world_tick_seconds
    }

    fn wind_volume_bucket_time(
        step_index: u32,
        bucket_index: u32,
        bucket_count: u32,
        step_seconds: f32,
    ) -> f32 {
        if step_index < bucket_index {
            return 0.0;
        }

        let last_bucket_step =
            bucket_index + ((step_index - bucket_index) / bucket_count) * bucket_count;
        last_bucket_step as f32 * step_seconds
    }

    fn record_wind_volume_bucket_pass(
        &self,
        cmdbuf: &CommandBuffer,
        dispatch_extent: Extent3D,
        bucket_index: u32,
        time: f32,
    ) {
        let push_constants = WindVolumePushConstants { time, bucket_index };

        self.compute_pipelines.wind_volume_ppl.record(
            cmdbuf,
            dispatch_extent,
            Some(bytemuck::bytes_of(&push_constants)),
        );
    }

    fn record_wind_volume_pass(&mut self, cmdbuf: &CommandBuffer, time: f32) {
        let bucket_count = WIND_VOLUME_BUCKET_COUNT;
        let step_seconds = self.wind_volume_bucket_step_seconds();
        let step_index = (time / step_seconds).floor().max(0.0) as u32;
        let mut dispatch_extent = self
            .resources
            .wind
            .wind_volume_tex
            .get_image()
            .get_desc()
            .extent;
        dispatch_extent.width /= WIND_VOLUME_BUCKET_COUNT;

        if self.initialized_wind_volume_bucket_count != bucket_count {
            for bucket_index in 0..bucket_count {
                let bucket_time = Self::wind_volume_bucket_time(
                    step_index,
                    bucket_index,
                    bucket_count,
                    step_seconds,
                );
                self.record_wind_volume_bucket_pass(
                    cmdbuf,
                    dispatch_extent,
                    bucket_index,
                    bucket_time,
                );
            }

            self.initialized_wind_volume_bucket_count = bucket_count;
            self.last_wind_volume_step = Some(step_index);
            return;
        }

        if self.last_wind_volume_step == Some(step_index) {
            return;
        }

        let bucket_index = step_index % bucket_count;
        let bucket_time = step_index as f32 * step_seconds;
        self.record_wind_volume_bucket_pass(cmdbuf, dispatch_extent, bucket_index, bucket_time);
        self.last_wind_volume_step = Some(step_index);
    }

    fn record_vsm_filtering_pass(
        &mut self,
        cmdbuf: &CommandBuffer,
        vsm_blur_radius: u32,
        vsm_temporal_alpha: f32,
        reset_vsm_history: bool,
    ) {
        let compute_to_compute_barrier = PipelineBarrier::compute_shader_access();

        let extent = self
            .resources
            .shadow
            .shadow_map_tex
            .get_image()
            .get_desc()
            .extent;
        self.compute_pipelines
            .vsm_creation_ppl
            .record(cmdbuf, extent, None);

        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        let reset_history = reset_vsm_history || !self.shadow_map_history_valid;
        let push_constants = VsmFilterPushConstants {
            blur_radius: vsm_blur_radius,
            temporal_alpha: vsm_temporal_alpha.clamp(0.0, 1.0),
            reset_history: u32::from(reset_history),
            _pad0: 0,
        };
        let push_constants_bytes = bytemuck::bytes_of(&push_constants);
        self.compute_pipelines
            .vsm_blur_h_ppl
            .record(cmdbuf, extent, Some(push_constants_bytes));

        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        self.compute_pipelines
            .vsm_blur_v_ppl
            .record(cmdbuf, extent, Some(push_constants_bytes));

        self.record_store_vsm_history(cmdbuf);
        self.shadow_map_history_valid = true;
    }

    fn record_tracer_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.tracer_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .compute_output_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn clear_tracer_outputs(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .compute_output_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::UInt([0, 0, 0, 0])),
            );
        self.resources
            .extent_dependent_resources
            .compute_depth_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
            );
    }

    fn record_god_ray_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.god_ray_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .compute_depth_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_denoiser_pass(
        &self,
        cmdbuf: &CommandBuffer,
        a_trous_iteration_count: u32,
    ) -> anyhow::Result<()> {
        // Validate iteration count - only 1, 3, or 5 are allowed
        if a_trous_iteration_count != 1
            && a_trous_iteration_count != 3
            && a_trous_iteration_count != 5
        {
            return Err(anyhow::anyhow!(
                "A-Trous iteration count must be 1, 3, or 5, got: {}",
                a_trous_iteration_count
            ));
        }
        let compute_to_compute_barrier = PipelineBarrier::compute_shader_access();

        let extent = self
            .resources
            .extent_dependent_resources
            .compute_output_tex
            .get_image()
            .get_desc()
            .extent;

        self.compute_pipelines
            .temporal_ppl
            .record(cmdbuf, extent, None);

        for i in 0..a_trous_iteration_count {
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            self.compute_pipelines.spatial_ppl.record(
                cmdbuf,
                self.resources
                    .extent_dependent_resources
                    .compute_output_tex
                    .get_image()
                    .get_desc()
                    .extent,
                Some(&i.to_ne_bytes()),
            );
        }

        Ok(())
    }

    fn record_cloud_shadow_pass(&mut self, cmdbuf: &CommandBuffer) {
        let extent = self
            .resources
            .shadow
            .cloud_shadow_raw_tex
            .get_image()
            .get_desc()
            .extent;

        self.compute_pipelines
            .cloud_shadow_ppl
            .record(cmdbuf, extent, None);

        PipelineBarrier::compute_shader_access().record_insert(self.vulkan_ctx.device(), cmdbuf);

        let push_constants = CloudShadowTemporalPushConstants {
            reset_history: u32::from(!self.cloud_shadow_history_valid),
        };
        self.compute_pipelines.cloud_shadow_temporal_ppl.record(
            cmdbuf,
            extent,
            Some(bytemuck::bytes_of(&push_constants)),
        );
        self.cloud_shadow_history_valid = true;
    }

    fn record_cloud_pass(&mut self, cmdbuf: &CommandBuffer) {
        let extent = self
            .resources
            .extent_dependent_resources
            .cloud_raw_tex
            .get_image()
            .get_desc()
            .extent;

        self.compute_pipelines
            .cloud_ppl
            .record(cmdbuf, extent, None);

        PipelineBarrier::compute_shader_access().record_insert(self.vulkan_ctx.device(), cmdbuf);

        let push_constants = CloudTemporalPushConstants {
            reset_history: u32::from(!self.cloud_history_valid),
        };
        self.compute_pipelines.cloud_temporal_ppl.record(
            cmdbuf,
            extent,
            Some(bytemuck::bytes_of(&push_constants)),
        );
        self.cloud_history_valid = true;
    }

    fn clear_cloud_output(&mut self, cmdbuf: &CommandBuffer) {
        self.cloud_history_valid = false;
        self.cloud_shadow_history_valid = false;
        for tex in [
            &self.resources.extent_dependent_resources.cloud_raw_tex,
            &self.resources.extent_dependent_resources.cloud_output_tex,
        ] {
            tex.get_image().record_clear(
                cmdbuf,
                Some(TextureLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 0.0])),
            );
        }
    }

    fn record_composition_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.composition_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .composited_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_lens_flare_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.lens_flare_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .lens_flare_full_output_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_lens_flare_sun_visible_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.lens_flare_sun_visible_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .lens_flare_full_output_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_lens_flare_downsample_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.lens_flare_downsample_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .lens_flare_output_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_post_processing_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines.post_processing_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .screen_output_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    #[allow(dead_code)]
    fn record_player_collider_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines
            .player_collider_ppl
            .record(cmdbuf, Extent3D::new(1, 1, 1), None);
    }

    pub fn handle_keyboard(&mut self, key_event: &KeyEvent) {
        self.camera.handle_keyboard(key_event);
    }

    pub fn reset_camera_input(&mut self) {
        self.camera.reset_input();
    }

    pub fn handle_mouse(&mut self, delta: Vec2) {
        self.camera.handle_mouse(delta);
    }

    pub fn reset_camera_velocity(&mut self) {
        self.camera.reset_velocity();
    }

    pub fn camera_position(&self) -> Vec3 {
        self.camera.position()
    }

    pub fn camera_pose(&self) -> CameraPose {
        self.camera.pose()
    }

    pub fn apply_camera_pose(&mut self, pose: CameraPose) {
        self.camera.apply_pose(pose);
        let view_mat = self.camera.get_view_mat();
        let proj_mat = self.camera.get_proj_mat();
        self.camera_view_mat_prev_frame = view_mat;
        self.camera_proj_mat_prev_frame = proj_mat;
        self.current_view_proj_mat = proj_mat * view_mat;
        self.shadow_map_history_valid = false;
        self.cloud_history_valid = false;
        if let Err(err) = self
            .spatial_sound_manager
            .update_player_pos(self.camera.position(), self.camera.vectors())
        {
            log::warn!(
                "Failed to update listener after applying camera pose: {}",
                err
            );
        }
    }

    pub fn camera_front(&self) -> Vec3 {
        self.camera.front()
    }

    pub fn project_screen_point_to_world(
        &self,
        screen_pos: Vec2,
        screen_extent: Vec2,
        distance_from_camera: f32,
    ) -> Option<Vec3> {
        if screen_extent.x <= 0.0 || screen_extent.y <= 0.0 {
            return None;
        }

        let ndc = Vec3::new(
            (screen_pos.x / screen_extent.x) * 2.0 - 1.0,
            (screen_pos.y / screen_extent.y) * 2.0 - 1.0,
            0.0,
        );
        let clip = ndc.extend(1.0);
        let mut world = self.current_view_proj_mat.inverse() * clip;
        if world.w.abs() <= 1e-6 {
            return None;
        }
        world /= world.w;

        let camera_pos = self.camera.position();
        let world_pos = Vec3::new(world.x, world.y, world.z);
        let direction = (world_pos - camera_pos).normalize_or_zero();
        if direction.length_squared() <= 1e-6 {
            return None;
        }

        Some(camera_pos + direction * distance_from_camera)
    }

    #[allow(dead_code)]
    pub fn camera_vectors(&self) -> &CameraVectors {
        self.camera.vectors()
    }

    pub fn set_footstep_volume_gain(&mut self, volume_gain: f32) {
        self.camera.set_footstep_volume_gain(volume_gain);
    }

    pub fn update_camera(&mut self, frame_delta_time: f32, _is_fly_mode: bool) {
        self.camera.update_transform(frame_delta_time);

        self.spatial_sound_manager
            .update_player_pos(self.camera.position(), self.camera.vectors())
            .unwrap();
    }

    pub fn upload_particles(&mut self, snapshots: &[ParticleSnapshot]) -> Result<()> {
        let capacity = PARTICLE_CAPACITY;
        let count = snapshots.len().min(capacity);
        let texture_layout = ParticleTextureLayout::new();
        texture_layout.assert_valid();
        let texture_layer_count = self
            .resources
            .textures
            .particle_lod_tex_lut
            .get_image()
            .get_desc()
            .array_len;
        debug_assert_eq!(
            texture_layer_count,
            texture_layout.total_layer_count(),
            "Particle texture LUT layer count mismatch (runtime {}, expected {})",
            texture_layer_count,
            texture_layout.total_layer_count()
        );
        let butterfly_frame_count = texture_layout.butterfly_frames_per_view();
        let camera_right_xz =
            Vec2::new(self.camera.vectors().right.x, self.camera.vectors().right.z)
                .normalize_or_zero();
        let camera_forward_xz =
            Vec2::new(self.camera.vectors().front.x, self.camera.vectors().front.z)
                .normalize_or_zero();
        const SPRITE_FLIP_BIT: u32 = 1 << 31;
        const MIN_SPEED_SQ: f32 = 0.01 * 0.01;

        let is_moving_right_relative_to_player = |velocity: Vec3| -> bool {
            let velocity_xz = Vec2::new(velocity.x, velocity.z);
            if velocity_xz.length_squared() <= MIN_SPEED_SQ {
                return false;
            }
            if camera_right_xz.length_squared() <= f32::EPSILON {
                return false;
            }

            velocity_xz.normalize().dot(camera_right_xz) > 0.0
        };
        let pack_particle_tex_index = |tex_index: u32, flip_sprite_x: bool| -> u32 {
            let base = tex_index & !SPRITE_FLIP_BIT;
            if flip_sprite_x {
                base | SPRITE_FLIP_BIT
            } else {
                base
            }
        };

        self.particle_instance_scratch.clear();
        self.particle_instance_scratch.reserve(count);
        for snap in snapshots.iter().take(capacity) {
            let butterfly_tex_index = {
                let vel_xz = Vec2::new(snap.velocity.x, snap.velocity.z);
                let vel_dir_xz = if vel_xz.length_squared() > MIN_SPEED_SQ {
                    vel_xz.normalize()
                } else {
                    Vec2::ZERO
                };
                let view_index = if vel_dir_xz == Vec2::ZERO {
                    0
                } else {
                    let z = vel_dir_xz.dot(camera_forward_xz);

                    if z >= 0.0 {
                        0
                    } else {
                        1
                    }
                };

                let palette_preset = ButterflyPalettePreset::from_index(snap.texture_variant);
                let preset_base_layer =
                    texture_layout.butterfly_preset_base_layer(palette_preset as u32);
                let frame_offset = snap.animation_frame_offset % butterfly_frame_count.max(1);
                let tex_index =
                    preset_base_layer + view_index * butterfly_frame_count + frame_offset;
                debug_assert!(
                    texture_layout.contains_layer(tex_index),
                    "Butterfly texture index {} out of LUT bounds {}",
                    tex_index,
                    texture_layout.total_layer_count()
                );
                tex_index
            };
            self.particle_instance_scratch.push(ParticleInstanceGpu {
                position: snap.position_ws.to_array(),
                size: snap.size,
                color: snap.color.to_array(),
                tex_index: match snap.kind {
                    crate::particles::ParticleRenderKind::Leaf => texture_layout.leaf_layer(),
                    crate::particles::ParticleRenderKind::Butterfly => pack_particle_tex_index(
                        butterfly_tex_index,
                        is_moving_right_relative_to_player(snap.velocity),
                    ),
                },
            });
        }

        if count > 0 {
            self.particle_resources
                .instance_buffer
                .fill(&self.particle_instance_scratch)?;
        }
        self.particle_resources.instance_count = count as u32;
        Ok(())
    }

    fn build_tree_render_instances(
        &self,
        tree_id: u32,
        positions: &[UVec3],
        aabb_margin: f32,
        span_label: &str,
    ) -> Result<TreeLeavesInstance> {
        use crate::builder::TreeLeafInstance;

        let mut instances_data = Vec::with_capacity(positions.len());
        let chunk_world_offset = positions
            .iter()
            .copied()
            .reduce(UVec3::min)
            .unwrap_or(UVec3::ZERO);
        if let Some(max_pos) = positions.iter().copied().reduce(UVec3::max) {
            anyhow::ensure!(
                (max_pos - chunk_world_offset)
                    .cmplt(UVec3::splat(1024))
                    .all(),
                "{} instance span exceeds packed 10-bit local-position range",
                span_label,
            );
        }
        let pack_local_pos = |world_pos: UVec3| -> u32 {
            let local_pos = world_pos - chunk_world_offset;
            (local_pos.x & 0x3ff) | ((local_pos.y & 0x3ff) << 10) | ((local_pos.z & 0x3ff) << 20)
        };
        for pos in positions.iter() {
            instances_data.push(TreeLeafInstance {
                packed_local_pos: pack_local_pos(*pos),
                packed_orientation: 0,
            });
        }

        let scaled_positions = positions
            .iter()
            .map(|pos| {
                Vec3::new(
                    pos.x as f32 / 256.0,
                    pos.y as f32 / 256.0,
                    pos.z as f32 / 256.0,
                )
            })
            .collect::<Vec<_>>();
        let aabb =
            crate::builder::InstanceResources::compute_leaves_aabb(&scaled_positions, aabb_margin);

        let mut tree_instance = TreeLeavesInstance::new_with_capacity(
            tree_id,
            aabb,
            chunk_world_offset,
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            positions.len() as u64,
        );

        if !instances_data.is_empty() {
            tree_instance
                .resources
                .instances_buf
                .fill(&instances_data)?;
            tree_instance.resources.instances_len = instances_data.len() as u32;
        } else {
            tree_instance.resources.instances_len = 0;
        }

        Ok(tree_instance)
    }

    pub fn add_tree_leaves(
        &mut self,
        surface_resources: &mut SurfaceResources,
        tree_id: u32,
        leaf_positions: &[UVec3],
    ) -> Result<()> {
        let tree_leaves_instance =
            self.build_tree_render_instances(tree_id, leaf_positions, 0.2, "tree leaf")?;
        surface_resources
            .instances
            .leaves_instances
            .insert(tree_id, tree_leaves_instance);

        Ok(())
    }

    pub fn add_tree_apples(
        &mut self,
        surface_resources: &mut SurfaceResources,
        tree_id: u32,
        apple_positions: &[UVec3],
    ) -> Result<()> {
        let tree_apple_instance =
            self.build_tree_render_instances(tree_id, apple_positions, 0.08, "tree apple")?;
        surface_resources
            .instances
            .apple_instances
            .insert(tree_id, tree_apple_instance);

        Ok(())
    }

    pub fn remove_tree_leaves(
        &mut self,
        surface_resources: &mut SurfaceResources,
        tree_id: u32,
    ) -> Result<()> {
        self.vulkan_ctx.device().wait_idle();
        let removed_leaves = surface_resources
            .instances
            .leaves_instances
            .remove(&tree_id);
        let removed_apples = surface_resources.instances.apple_instances.remove(&tree_id);

        match (removed_leaves, removed_apples) {
            (Some(leaves), Some(apples)) => log::info!(
                "Removed tree {} with {} leaves and {} apples",
                tree_id,
                leaves.resources.instances_len,
                apples.resources.instances_len
            ),
            (Some(leaves), None) => log::info!(
                "Removed tree {} with {} leaves",
                tree_id,
                leaves.resources.instances_len
            ),
            (None, Some(apples)) => log::info!(
                "Removed tree {} with {} apples",
                tree_id,
                apples.resources.instances_len
            ),
            (None, None) => log::warn!("Attempted to remove non-existent tree {}", tree_id),
        }
        Ok(())
    }

    pub fn regenerate_leaves(
        &mut self,
        inner_density: f32,
        outer_density: f32,
        inner_radius: f32,
        outer_radius: f32,
    ) -> Result<()> {
        let device = self.vulkan_ctx.device();
        self.resources.meshes.leaves_resources = LeavesResources::new_with_params(
            device.clone(),
            self.allocator.clone(),
            inner_density,
            outer_density,
            inner_radius,
            outer_radius,
            false,
        );

        self.resources.meshes.leaves_resources_lod = LeavesResources::new_with_params(
            device.clone(),
            self.allocator.clone(),
            inner_density,
            outer_density,
            inner_radius,
            outer_radius,
            true,
        );
        Ok(())
    }

    pub fn query_terrain_rays_batch_with_validity(
        &mut self,
        rays: &[TerrainRayQuery],
    ) -> Result<Vec<TerrainRayHitSample>> {
        if rays.is_empty() {
            return Ok(vec![]);
        }

        let mut all_hits = Vec::with_capacity(rays.len());
        for chunk in rays.chunks(MAX_TERRAIN_QUERIES) {
            all_hits.extend(self.query_terrain_rays_chunk_with_validity(chunk)?);
        }
        Ok(all_hits)
    }

    fn query_terrain_rays_chunk_with_validity(
        &mut self,
        rays: &[TerrainRayQuery],
    ) -> Result<Vec<TerrainRayHitSample>> {
        debug_assert!(!rays.is_empty());
        debug_assert!(rays.len() <= MAX_TERRAIN_QUERIES);

        let query_count = rays.len() as u32;

        self.resources
            .terrain_query
            .terrain_query_count
            .fill_uniform(&crate::generated::gpu_structs::TerrainQueryCount {
                valid_query_count: query_count,
                ..bytemuck::Zeroable::zeroed()
            })?;

        let mut ray_data = Vec::with_capacity(rays.len() * 8);
        for ray in rays {
            ray_data.push(ray.origin.x);
            ray_data.push(ray.origin.y);
            ray_data.push(ray.origin.z);
            ray_data.push(0.0);
            ray_data.push(ray.direction.x);
            ray_data.push(ray.direction.y);
            ray_data.push(ray.direction.z);
            ray_data.push(0.0);
        }
        self.resources
            .terrain_query
            .terrain_query_info
            .fill(&ray_data)?;

        execute_one_time_gpu_job(
            self.vulkan_ctx.device(),
            self.vulkan_ctx.command_pool(),
            &self.vulkan_ctx.get_general_queue(),
            |cmdbuf| {
                self.compute_pipelines.terrain_query_ppl.record(
                    cmdbuf,
                    Extent3D::new(query_count, 1, 1),
                    None,
                );
            },
        );

        let raw_data = self
            .resources
            .terrain_query
            .terrain_query_result
            .read_back()
            .unwrap();
        let hit_data: &[f32] = unsafe {
            std::slice::from_raw_parts(raw_data.as_ptr() as *const f32, (query_count as usize) * 4)
        };

        let mut hits = Vec::with_capacity(query_count as usize);
        for item in hit_data.chunks_exact(4) {
            hits.push(TerrainRayHitSample {
                position: Vec3::new(item[0], item[1], item[2]),
                is_valid: item[3] >= 0.5,
            });
        }
        Ok(hits)
    }

    pub fn query_terrain_ray_with_validity(
        &mut self,
        ray: TerrainRayQuery,
    ) -> Result<TerrainRayHitSample> {
        let samples = self.query_terrain_rays_batch_with_validity(&[ray])?;
        Ok(samples[0])
    }
}
