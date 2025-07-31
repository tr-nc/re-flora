mod resources;
use bytemuck::{Pod, Zeroable};
pub use resources::*;

mod denoiser_resources;
pub use denoiser_resources::*;

mod extent_dependent_resources;
pub use extent_dependent_resources::*;

mod vertex;
pub use vertex::*;

mod voxel_encoding;

mod voxel_geometry;

mod flora_construct;

mod leaves_construct;

mod pipeline_builder;
use pipeline_builder::*;

mod buffer_updater;
use buffer_updater::*;

use glam::{Mat4, UVec3, Vec2, Vec3};
use winit::event::KeyEvent;

use crate::audio::{spatial_sound_calculator::SpatialSoundCalculator, AudioEngine};
use crate::builder::{
    ContreeBuilderResources, FloraInstanceResources, FloraType, Instance,
    SceneAccelBuilderResources, SurfaceResources, TreeLeavesInstance,
};
use crate::gameplay::{calculate_directional_light_matrices, Camera, CameraDesc, CameraVectors};
use crate::geom::UAabb3;
use crate::resource::ResourceContainer;
use crate::util::{ShaderCompiler, TimeInfo};
use crate::vkn::{
    execute_one_time_command, Allocator, Buffer, ClearValue, ColorClearValue, CommandBuffer,
    ComputePipeline, DepthOrStencilClearValue, DescriptorPool, Extent2D, Extent3D, Framebuffer,
    GraphicsPipeline, MemoryBarrier, PipelineBarrier, PlainMemberTypeWithData, PushConstantInfo,
    RenderPass, RenderTarget, StructMemberDataBuilder, StructMemberDataReader, Texture, Viewport,
    VulkanContext,
};
use anyhow::Result;
use ash::vk;
use std::collections::HashMap;

#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
struct PushConstantStd140 {
    time: f32,
    // `std140` requires a `vec3` to be aligned to 16 bytes.
    // `time` is 4 bytes, so we need 12 bytes of padding to reach offset 16.
    _padding1: [u8; 12],

    bottom_color: Vec3,
    // After `bottom_color` (12 bytes), we are at offset 16 + 12 = 28.
    // The next field (`tip_color`) must also start on a 16-byte boundary (offset 32).
    // So we need 4 bytes of padding.
    _padding2: [u8; 4],

    tip_color: Vec3,
    // The total size of the block must be a multiple of 16.
    // We are at offset 32 + 12 = 44. The next multiple of 16 is 48.
    // So we need 4 final bytes of padding.
    _padding3: [u8; 4],
}

impl PushConstantStd140 {
    pub fn new(time: f32, bottom_color: Vec3, tip_color: Vec3) -> Self {
        Self {
            time,
            _padding1: [0; 12],
            bottom_color,
            _padding2: [0; 4],
            tip_color,
            _padding3: [0; 4],
        }
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

#[derive(Debug, Clone)]
pub struct PlayerCollisionResult {
    pub ground_distance: f32,
    pub ring_distances: Vec<f32>,
}

pub struct Tracer {
    vulkan_ctx: VulkanContext,

    desc: TracerDesc,
    chunk_bound: UAabb3,

    allocator: Allocator,
    resources: TracerResources,

    camera: Camera,
    camera_view_mat_prev_frame: Mat4,
    camera_proj_mat_prev_frame: Mat4,
    current_view_proj_mat: Mat4,
    current_shadow_view_proj_mat: Mat4,

    compute_pipelines: ComputePipelines,
    graphics_pipelines: GraphicsPipelines,

    render_target_color_and_depth: RenderTarget,
    render_target_depth_only: RenderTarget,

    #[allow(dead_code)]
    pool: DescriptorPool,

    a_trous_iteration_count: u32,
    spatial_sound_calculator: Option<SpatialSoundCalculator>,
}

impl Drop for Tracer {
    fn drop(&mut self) {}
}

impl Tracer {
    pub fn new(
        vulkan_ctx: VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
        chunk_bound: UAabb3,
        screen_extent: Extent2D,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
        desc: TracerDesc,
        audio_engine: AudioEngine,
    ) -> Result<Self> {
        let render_extent = Self::get_render_extent(screen_extent, desc.scaling_factor);

        let camera = Camera::new(
            Vec3::new(0.5, 0.8, 0.5),
            135.0,
            -5.0,
            CameraDesc {
                aspect_ratio: render_extent.get_aspect_ratio(),
                ..Default::default()
            },
            audio_engine,
        )?;

        let pool = DescriptorPool::new(vulkan_ctx.device()).unwrap();

        let shader_modules = PipelineBuilder::create_shader_modules(&vulkan_ctx, &shader_compiler)?;

        let resources = TracerResources::new(
            &vulkan_ctx,
            allocator.clone(),
            &shader_modules.tracer_sm,
            &shader_modules.tracer_shadow_sm,
            &shader_modules.composition_sm,
            &shader_modules.temporal_sm,
            &shader_modules.spatial_sm,
            &shader_modules.taa_sm,
            &shader_modules.god_ray_sm,
            &shader_modules.post_processing_sm,
            &shader_modules.player_collider_sm,
            &shader_modules.terrain_query_sm,
            render_extent,
            screen_extent,
            Extent2D::new(1024, 1024),
            1000, // max_terrain_queries
        );

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
            resources.shadow_map_tex.clone(),
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
            &resources.shadow_map_tex,
        );

        let render_target_color_and_depth = RenderTarget::new(
            render_passes.render_pass_color_and_depth,
            vec![framebuffer_color_and_depth],
        );
        let render_target_depth_only = RenderTarget::new(
            render_passes.render_pass_depth,
            vec![framebuffer_depth_only],
        );

        return Ok(Self {
            vulkan_ctx,
            desc,
            chunk_bound,
            allocator,
            resources,
            camera,
            camera_view_mat_prev_frame: Mat4::IDENTITY,
            camera_proj_mat_prev_frame: Mat4::IDENTITY,
            current_view_proj_mat: Mat4::IDENTITY,
            current_shadow_view_proj_mat: Mat4::IDENTITY,
            compute_pipelines,
            graphics_pipelines,
            render_target_color_and_depth,
            render_target_depth_only,
            pool,
            a_trous_iteration_count: 3,
            spatial_sound_calculator: None,
        });
    }

    /// A framebuffer that contains the color and depth textures for the main render pass
    fn create_framebuffer_color_and_depth(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        target_texture: &Texture,
        depth_texture: &Texture,
    ) -> Framebuffer {
        let target_view = target_texture.get_image_view().as_raw();
        let depth_image_view = depth_texture.get_image_view().as_raw();

        let target_image_extent = target_texture
            .get_image()
            .get_desc()
            .extent
            .as_extent_2d()
            .unwrap();

        Framebuffer::new(
            vulkan_ctx.clone(),
            &render_pass,
            &[target_view, depth_image_view],
            target_image_extent,
        )
        .unwrap()
    }

    /// A framebuffer that contains the shadow map texture
    fn create_framebuffer_depth(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        shadow_map_tex: &Texture,
    ) -> Framebuffer {
        let shadow_image_view = shadow_map_tex.get_image_view().as_raw();
        let shadow_image_extent = shadow_map_tex
            .get_image()
            .get_desc()
            .extent
            .as_extent_2d()
            .unwrap();
        Framebuffer::new(
            vulkan_ctx.clone(),
            &render_pass,
            &[shadow_image_view],
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
            &self.resources.shadow_map_tex,
        );

        self.render_target_color_and_depth = RenderTarget::new(
            self.render_target_color_and_depth.get_render_pass().clone(),
            vec![framebuffer_color_and_depth],
        );
        self.render_target_depth_only = RenderTarget::new(
            self.render_target_depth_only.get_render_pass().clone(),
            vec![framebuffer_depth_only],
        );

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

        // pipelines that need all resources (tracer, scene_accel, contree)
        let all_resources = &[
            &self.resources as &dyn ResourceContainer,
            contree_builder_resources as &dyn ResourceContainer,
            scene_accel_resources as &dyn ResourceContainer,
        ];
        update_compute_fn(&self.compute_pipelines.tracer_ppl, all_resources);
        update_compute_fn(&self.compute_pipelines.tracer_shadow_ppl, all_resources);
        update_compute_fn(&self.compute_pipelines.player_collider_ppl, all_resources);
        update_compute_fn(&self.compute_pipelines.terrain_query_ppl, all_resources);

        // pipelines that only need tracer resources
        let tracer_resources = &[&self.resources as &dyn ResourceContainer];
        update_compute_fn(&self.compute_pipelines.vsm_creation_ppl, tracer_resources);
        update_compute_fn(&self.compute_pipelines.vsm_blur_h_ppl, tracer_resources);
        update_compute_fn(&self.compute_pipelines.vsm_blur_v_ppl, tracer_resources);
        update_compute_fn(&self.compute_pipelines.god_ray_ppl, tracer_resources);
        update_compute_fn(&self.compute_pipelines.temporal_ppl, tracer_resources);
        update_compute_fn(&self.compute_pipelines.spatial_ppl, tracer_resources);
        update_compute_fn(&self.compute_pipelines.composition_ppl, tracer_resources);
        update_compute_fn(&self.compute_pipelines.taa_ppl, tracer_resources);
        update_compute_fn(
            &self.compute_pipelines.post_processing_ppl,
            tracer_resources,
        );

        // update graphics pipelines descriptor sets
        update_graphics_fn(&self.graphics_pipelines.flora_ppl, tracer_resources);
        update_graphics_fn(&self.graphics_pipelines.flora_lod_ppl, tracer_resources);
        update_graphics_fn(
            &self.graphics_pipelines.leaves_shadow_lod_ppl,
            tracer_resources,
        );
    }

    // create a lower resolution texture for rendering, for better performance,
    // less memory usage, and stylized rendering
    fn get_render_extent(screen_extent: Extent2D, scaling_factor: f32) -> Extent2D {
        let extent = Extent2D::new(
            (screen_extent.width as f32 * scaling_factor) as u32,
            (screen_extent.height as f32 * scaling_factor) as u32,
        );
        extent
    }

    pub fn get_screen_output_tex(&self) -> &Texture {
        &self.resources.extent_dependent_resources.screen_output_tex
    }

    pub fn update_buffers(
        &mut self,
        time_info: &TimeInfo,
        debug_float: f32,
        debug_bool: bool,
        debug_uint: u32,
        sun_dir: Vec3,
        sun_size: f32,
        sun_color: Vec3,
        sun_luminance: f32,
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
        is_taa_enabled: bool,
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
        voxel_sand_color: Vec3,
        voxel_dirt_color: Vec3,
        voxel_rock_color: Vec3,
        voxel_leaf_color: Vec3,
        voxel_trunk_color: Vec3,
    ) -> Result<()> {
        // camera info
        let view_mat = self.camera.get_view_mat();
        let proj_mat = self.camera.get_proj_mat();
        self.current_view_proj_mat = proj_mat * view_mat;
        BufferUpdater::update_camera_info(&mut self.resources.camera_info, view_mat, proj_mat)?;

        // shadow cam info
        let world_bound = self.chunk_bound.into();
        let (shadow_view_mat, shadow_proj_mat) =
            calculate_directional_light_matrices(world_bound, sun_dir);
        self.current_shadow_view_proj_mat = shadow_proj_mat * shadow_view_mat;
        BufferUpdater::update_camera_info(
            &mut self.resources.shadow_camera_info,
            shadow_view_mat,
            shadow_proj_mat,
        )?;

        // camera info prev frame
        BufferUpdater::update_camera_info(
            &mut self.resources.camera_info_prev_frame,
            self.camera_view_mat_prev_frame,
            self.camera_proj_mat_prev_frame,
        )?;

        BufferUpdater::update_taa_info(&self.resources, is_taa_enabled)?;

        BufferUpdater::update_god_ray_info(
            &self.resources,
            god_ray_max_depth,
            god_ray_max_checks,
            god_ray_weight,
            god_ray_color,
        )?;

        BufferUpdater::update_post_processing_info(&self.resources, self.desc.scaling_factor)?;

        BufferUpdater::update_player_collider_info(
            &self.resources,
            self.camera.position(),
            self.camera.front(),
        )?;

        BufferUpdater::update_voxel_colors(
            &self.resources,
            voxel_sand_color,
            voxel_dirt_color,
            voxel_rock_color,
            voxel_leaf_color,
            voxel_trunk_color,
        )?;

        BufferUpdater::update_gui_input(&self.resources, debug_float, debug_bool, debug_uint)?;

        BufferUpdater::update_sun_info(
            &self.resources,
            sun_dir,
            sun_size,
            sun_color,
            sun_luminance,
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
        surface_resources: &'a SurfaceResources,
        lod_distance: f32,
    ) -> HashMap<LodState, Vec<&'a TreeLeavesInstance>> {
        let mut lod0_instances = Vec::new();
        let mut lod1_instances = Vec::new();
        let camera_pos = self.camera.position();

        for (_tree_id, tree_instance) in &surface_resources.instances.leaves_instances {
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

    pub fn record_trace(
        &mut self,
        cmdbuf: &CommandBuffer,
        surface_resources: &SurfaceResources,
        lod_distance: f32,
        time: f32,
        grass_bottom_color: Vec3,
        grass_tip_color: Vec3,
        lavender_bottom_color: Vec3,
        lavender_tip_color: Vec3,
        leaf_bottom_color: Vec3,
        leaf_tip_color: Vec3,
    ) -> Result<()> {
        let shader_access_memory_barrier = MemoryBarrier::new_shader_access();
        let compute_to_compute_barrier = PipelineBarrier::new(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![shader_access_memory_barrier],
        );
        let frag_to_vert_barrier = PipelineBarrier::new(
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::VERTEX_SHADER,
            vec![shader_access_memory_barrier],
        );

        self.record_clear_render_targets(cmdbuf);

        self.record_leaves_shadow_lod_pass(
            cmdbuf,
            surface_resources,
            leaf_bottom_color,
            leaf_tip_color,
            time,
        );
        let frag_to_compute_barrier = PipelineBarrier::new(
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![shader_access_memory_barrier],
        );
        frag_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        self.record_tracer_shadow_pass(cmdbuf);
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        self.record_vsm_filtering_pass(cmdbuf);
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        let b1 = PipelineBarrier::new(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::VERTEX_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![shader_access_memory_barrier],
        );
        b1.record_insert(self.vulkan_ctx.device(), cmdbuf);

        let chunks_by_lod = self.chunks_needs_to_draw_this_frame(surface_resources, lod_distance);
        self.record_flora_pass(
            cmdbuf,
            &chunks_by_lod[&LodState::Lod0],
            LodState::Lod0,
            FloraType::Grass,
            grass_bottom_color,
            grass_tip_color,
            time,
        );
        frag_to_vert_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        self.record_flora_pass(
            cmdbuf,
            &chunks_by_lod[&LodState::Lod1],
            LodState::Lod1,
            FloraType::Grass,
            grass_bottom_color,
            grass_tip_color,
            time,
        );
        frag_to_vert_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        self.record_flora_pass(
            cmdbuf,
            &chunks_by_lod[&LodState::Lod0],
            LodState::Lod0,
            FloraType::Lavender,
            lavender_bottom_color,
            lavender_tip_color,
            time,
        );
        frag_to_vert_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        self.record_flora_pass(
            cmdbuf,
            &chunks_by_lod[&LodState::Lod1],
            LodState::Lod1,
            FloraType::Lavender,
            lavender_bottom_color,
            lavender_tip_color,
            time,
        );
        frag_to_vert_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        let trees_by_lod = self.trees_needs_to_draw_this_frame(surface_resources, lod_distance);
        self.record_leaves_pass(
            cmdbuf,
            &trees_by_lod[&LodState::Lod0],
            LodState::Lod0,
            leaf_bottom_color,
            leaf_tip_color,
            time,
        );
        frag_to_vert_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        self.record_leaves_pass(
            cmdbuf,
            &trees_by_lod[&LodState::Lod1],
            LodState::Lod1,
            leaf_bottom_color,
            leaf_tip_color,
            time,
        );
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        record_denoiser_resources_transition_barrier(&self.resources.denoiser_resources, cmdbuf);

        self.record_tracer_pass(cmdbuf);

        let b2 = PipelineBarrier::new(
            vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![shader_access_memory_barrier],
        );
        b2.record_insert(self.vulkan_ctx.device(), cmdbuf);

        self.record_god_ray_pass(cmdbuf);
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        self.record_denoiser_pass(&cmdbuf, self.a_trous_iteration_count)?;

        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        self.record_composition_pass(cmdbuf);
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        self.record_taa_pass(cmdbuf);
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        self.record_post_processing_pass(cmdbuf);
        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        self.record_player_collider_pass(cmdbuf);

        copy_current_to_prev(&self.resources, cmdbuf);

        return Ok(());

        fn record_denoiser_resources_transition_barrier(
            denoiser_resources: &DenoiserResources,
            cmdbuf: &CommandBuffer,
        ) {
            let tr_fn = |tex: &Texture| {
                tex.get_image()
                    .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);
            };
            tr_fn(&denoiser_resources.tex.denoiser_normal_tex);
            tr_fn(&denoiser_resources.tex.denoiser_normal_tex_prev);
            tr_fn(&denoiser_resources.tex.denoiser_position_tex);
            tr_fn(&denoiser_resources.tex.denoiser_position_tex_prev);
            tr_fn(&denoiser_resources.tex.denoiser_vox_id_tex);
            tr_fn(&denoiser_resources.tex.denoiser_vox_id_tex_prev);
            tr_fn(&denoiser_resources.tex.denoiser_accumed_tex);
            tr_fn(&denoiser_resources.tex.denoiser_accumed_tex_prev);
            tr_fn(&denoiser_resources.tex.denoiser_motion_tex);
            tr_fn(&denoiser_resources.tex.denoiser_temporal_hist_len_tex);
            tr_fn(&denoiser_resources.tex.denoiser_hit_tex);
            tr_fn(&denoiser_resources.tex.denoiser_spatial_ping_tex);
            tr_fn(&denoiser_resources.tex.denoiser_spatial_pong_tex);
        }

        fn copy_current_to_prev(resources: &TracerResources, cmdbuf: &CommandBuffer) {
            let copy_fn = |src_tex: &Texture, dst_tex: &Texture| {
                src_tex.get_image().record_copy_to(
                    cmdbuf,
                    dst_tex.get_image(),
                    vk::ImageLayout::GENERAL,
                    vk::ImageLayout::GENERAL,
                );
            };
            copy_fn(
                &resources.denoiser_resources.tex.denoiser_normal_tex,
                &resources.denoiser_resources.tex.denoiser_normal_tex_prev,
            );
            copy_fn(
                &resources.denoiser_resources.tex.denoiser_position_tex,
                &resources.denoiser_resources.tex.denoiser_position_tex_prev,
            );
            copy_fn(
                &resources.denoiser_resources.tex.denoiser_vox_id_tex,
                &resources.denoiser_resources.tex.denoiser_vox_id_tex_prev,
            );
            copy_fn(
                &resources.denoiser_resources.tex.denoiser_accumed_tex,
                &resources.denoiser_resources.tex.denoiser_accumed_tex_prev,
            );
            copy_fn(
                &resources.extent_dependent_resources.taa_tex,
                &resources.extent_dependent_resources.taa_tex_prev,
            );
        }
    }

    fn record_clear_render_targets(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .gfx_output_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(vk::ImageLayout::GENERAL),
                0,
                ClearValue::Color(ColorClearValue::Float([0.0, 0.0, 0.0, 1.0])),
            );
        self.resources
            .extent_dependent_resources
            .gfx_depth_tex
            .get_image()
            .record_clear(
                cmdbuf,
                Some(vk::ImageLayout::GENERAL),
                0,
                ClearValue::DepthStencil(DepthOrStencilClearValue::Depth(1.0)),
            );

        self.resources.shadow_map_tex.get_image().record_clear(
            cmdbuf,
            Some(vk::ImageLayout::GENERAL),
            0,
            ClearValue::DepthStencil(DepthOrStencilClearValue::Depth(1.0)),
        );
    }

    fn record_flora_pass(
        &self,
        cmdbuf: &CommandBuffer,
        flora_instances: &[&FloraInstanceResources],
        lod_state: LodState,
        flora_type: FloraType,
        bottom_color: Vec3,
        tip_color: Vec3,
        time: f32,
    ) {
        let pipeline = match lod_state {
            LodState::Lod0 => &self.graphics_pipelines.flora_ppl,
            LodState::Lod1 => &self.graphics_pipelines.flora_lod_ppl,
        };

        let render_target = &self.render_target_color_and_depth;

        let push_constant = PushConstantStd140::new(time, bottom_color, tip_color);

        let (indices_buf, vertices_buf, indices_len) = match flora_type {
            FloraType::Grass => (
                &self.resources.grass_blade_resources.indices,
                &self.resources.grass_blade_resources.vertices,
                self.resources.grass_blade_resources.indices_len,
            ),
            FloraType::Lavender => (
                &self.resources.lavender_resources.indices,
                &self.resources.lavender_resources.vertices,
                self.resources.lavender_resources.indices_len,
            ),
        };

        pipeline.record_bind(cmdbuf);

        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        render_target.record_begin(cmdbuf, &clear_values);

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

        pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);

        unsafe {
            self.vulkan_ctx.device().cmd_bind_index_buffer(
                cmdbuf.as_raw(),
                indices_buf.as_raw(),
                0,
                vk::IndexType::UINT32,
            );
        }

        for instances in flora_instances {
            let instances_buf = &instances.get(flora_type).instances_buf;
            let instances_len = instances.get(flora_type).instances_len;

            // only draw if this chunk actually has grass instances.
            if instances_len == 0 {
                continue;
            }

            // bind the vertex buffers for this specific chunk.
            // binding point 0: common grass blade vertices.
            // binding point 1: per-chunk, per-instance data.
            unsafe {
                self.vulkan_ctx.device().cmd_bind_vertex_buffers(
                    cmdbuf.as_raw(),
                    0, // firstBinding
                    &[vertices_buf.as_raw(), instances_buf.as_raw()],
                    &[0, 0], // offsets
                );
            }

            // issue the draw call for the current chunk.
            // no barriers are needed here.
            pipeline.record_indexed(
                cmdbuf,
                indices_len,
                instances_len,
                0, // firstIndex
                0, // vertexOffset
                0, // firstInstance
                Some(&PushConstantInfo {
                    shader_stage: vk::ShaderStageFlags::VERTEX,
                    push_constants: bytemuck::bytes_of(&push_constant).to_vec(),
                }),
            );
        }
        render_target.record_end(cmdbuf);

        let desc = render_target.get_desc();
        self.resources
            .extent_dependent_resources
            .gfx_output_tex
            .get_image()
            .set_layout(0, desc.attachments[0].final_layout);
        self.resources
            .extent_dependent_resources
            .gfx_depth_tex
            .get_image()
            .set_layout(0, desc.attachments[1].final_layout);
    }

    fn record_leaves_pass(
        &self,
        cmdbuf: &CommandBuffer,
        leaves_instances: &[&TreeLeavesInstance],
        lod_state: LodState,
        bottom_color: Vec3,
        tip_color: Vec3,
        time: f32,
    ) {
        // skip rendering entirely if no leaf instances exist
        if leaves_instances.is_empty() {
            return;
        }

        let pipeline = match lod_state {
            LodState::Lod0 => &self.graphics_pipelines.flora_ppl,
            LodState::Lod1 => &self.graphics_pipelines.flora_lod_ppl,
        };

        let render_target = &self.render_target_color_and_depth;

        let push_constant = PushConstantStd140::new(time, bottom_color, tip_color);

        let (indices_buf, vertices_buf, indices_len) = match lod_state {
            LodState::Lod0 => (
                &self.resources.leaves_resources.indices,
                &self.resources.leaves_resources.vertices,
                self.resources.leaves_resources.indices_len,
            ),
            LodState::Lod1 => (
                &self.resources.leaves_resources_lod.indices,
                &self.resources.leaves_resources_lod.vertices,
                self.resources.leaves_resources_lod.indices_len,
            ),
        };

        pipeline.record_bind(cmdbuf);

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

        render_target.record_begin(cmdbuf, &clear_values);

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

        pipeline.record_viewport_scissor(cmdbuf, viewport, scissor);

        unsafe {
            // bind the index buffer for leaves
            self.vulkan_ctx.device().cmd_bind_index_buffer(
                cmdbuf.as_raw(),
                indices_buf.as_raw(),
                0,
                vk::IndexType::UINT32,
            );
        }

        // loop through all tree leaves instances for this LOD level
        for tree_instance in leaves_instances {
            if tree_instance.resources.instances_len == 0 {
                continue;
            }

            // bind vertex buffers for this instance
            unsafe {
                self.vulkan_ctx.device().cmd_bind_vertex_buffers(
                    cmdbuf.as_raw(),
                    0,
                    &[
                        vertices_buf.as_raw(),
                        tree_instance.resources.instances_buf.as_raw(),
                    ],
                    &[0, 0],
                );
            }

            // render this instance
            pipeline.record_indexed(
                cmdbuf,
                indices_len,
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

        render_target.record_end(cmdbuf);

        let desc = render_target.get_desc();
        self.resources
            .extent_dependent_resources
            .gfx_output_tex
            .get_image()
            .set_layout(0, desc.attachments[0].final_layout);
        self.resources
            .extent_dependent_resources
            .gfx_depth_tex
            .get_image()
            .set_layout(0, desc.attachments[1].final_layout);
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
            .record_bind(cmdbuf);

        let push_constant = PushConstantStd140::new(time, bottom_color, tip_color);

        let clear_values = [vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.0,
                stencil: 0,
            },
        }];

        self.render_target_depth_only
            .record_begin(cmdbuf, &clear_values);

        let shadow_extent = self.resources.shadow_map_tex.get_image().get_desc().extent;
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

        unsafe {
            self.vulkan_ctx.device().cmd_bind_index_buffer(
                cmdbuf.as_raw(),
                self.resources.leaves_resources_lod.indices.as_raw(),
                0,
                vk::IndexType::UINT32,
            );
        }

        // loop through all tree leaves instances
        for (_tree_id, tree_instance) in &surface_resources.instances.leaves_instances {
            if tree_instance.resources.instances_len == 0 {
                continue;
            }

            unsafe {
                self.vulkan_ctx.device().cmd_bind_vertex_buffers(
                    cmdbuf.as_raw(),
                    0,
                    &[
                        self.resources.leaves_resources_lod.vertices.as_raw(),
                        tree_instance.resources.instances_buf.as_raw(),
                    ],
                    &[0, 0],
                );
            }

            // render this instance for shadow map
            self.graphics_pipelines
                .leaves_shadow_lod_ppl
                .record_indexed(
                    cmdbuf,
                    self.resources.leaves_resources_lod.indices_len,
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

        self.render_target_depth_only.record_end(cmdbuf);

        let desc = self.render_target_depth_only.get_desc();
        self.resources
            .shadow_map_tex
            .get_image()
            .set_layout(0, desc.attachments[0].final_layout);
    }

    fn record_tracer_shadow_pass(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .shadow_map_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);
        self.compute_pipelines.tracer_shadow_ppl.record(
            cmdbuf,
            self.resources.shadow_map_tex.get_image().get_desc().extent,
            None,
        );
    }

    fn record_vsm_filtering_pass(&self, cmdbuf: &CommandBuffer) {
        // transition shadow map to general
        self.resources
            .shadow_map_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);
        self.resources
            .shadow_map_tex_for_vsm_ping
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);
        self.resources
            .shadow_map_tex_for_vsm_pong
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);

        let shader_access_memory_barrier = MemoryBarrier::new_shader_access();
        let compute_to_compute_barrier = PipelineBarrier::new(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![shader_access_memory_barrier],
        );

        let extent = self.resources.shadow_map_tex.get_image().get_desc().extent;
        self.compute_pipelines
            .vsm_creation_ppl
            .record(cmdbuf, extent, None);

        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        self.compute_pipelines
            .vsm_blur_h_ppl
            .record(cmdbuf, extent, None);

        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        self.compute_pipelines
            .vsm_blur_v_ppl
            .record(cmdbuf, extent, None);
    }

    fn record_tracer_pass(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .compute_output_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);
        self.resources
            .extent_dependent_resources
            .compute_depth_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);

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

    fn record_god_ray_pass(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .god_ray_output_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);

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
        let shader_access_memory_barrier = MemoryBarrier::new_shader_access();
        let compute_to_compute_barrier = PipelineBarrier::new(
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vec![shader_access_memory_barrier],
        );

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

    fn record_composition_pass(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .composited_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);

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

    fn record_taa_pass(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .taa_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);
        self.resources
            .extent_dependent_resources
            .taa_tex_prev
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);

        self.compute_pipelines.taa_ppl.record(
            cmdbuf,
            self.resources
                .extent_dependent_resources
                .taa_tex
                .get_image()
                .get_desc()
                .extent,
            None,
        );
    }

    fn record_post_processing_pass(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .screen_output_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);

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

    fn record_player_collider_pass(&self, cmdbuf: &CommandBuffer) {
        self.compute_pipelines
            .player_collider_ppl
            .record(cmdbuf, Extent3D::new(1, 1, 1), None);
    }

    pub fn handle_keyboard(&mut self, key_event: &KeyEvent) {
        self.camera.handle_keyboard(key_event);
    }

    pub fn handle_mouse(&mut self, delta: Vec2) {
        self.camera.handle_mouse(delta);
    }

    pub fn reset_camera_velocity(&mut self) {
        self.camera.reset_velocity();
    }

    pub fn camera_vectors(&self) -> &CameraVectors {
        self.camera.vectors()
    }

    pub fn set_spatial_sound_calculator(
        &mut self,
        spatial_sound_calculator: SpatialSoundCalculator,
    ) {
        self.spatial_sound_calculator = Some(spatial_sound_calculator);
    }

    pub fn update_camera(&mut self, frame_delta_time: f32, is_fly_mode: bool) {
        if is_fly_mode {
            self.camera.update_transform_fly_mode(frame_delta_time);
        } else {
            let collision_result =
                get_player_collision_result(&self.resources.player_collision_result).unwrap();
            self.camera
                .update_transform_walk_mode(frame_delta_time, collision_result);
        }

        // update spatial sound calculator with camera position
        if let Some(ref spatial_sound_calculator) = self.spatial_sound_calculator {
            spatial_sound_calculator
                .update_player_pos(self.camera.position(), self.camera.vectors());
        }

        fn get_player_collision_result(
            player_collision_result: &Buffer,
        ) -> Result<PlayerCollisionResult> {
            let layout = &player_collision_result.get_layout().unwrap().root_member;
            let raw_data = player_collision_result.read_back().unwrap();
            let reader = StructMemberDataReader::new(layout, &raw_data);

            let ground_distance = if let PlainMemberTypeWithData::Float(val) =
                reader.get_field("ground_distance").unwrap()
            {
                val
            } else {
                panic!("Expected Float type for ground_distance");
            };

            let ring_distances = if let PlainMemberTypeWithData::Array(val) =
                reader.get_field("ring_distances").unwrap()
            {
                val
            } else {
                panic!("Expected Array type for ring_distances");
            };

            Ok(PlayerCollisionResult {
                ground_distance,
                ring_distances,
            })
        }
    }

    pub fn add_tree_leaves(
        &mut self,
        surface_resources: &mut SurfaceResources,
        tree_id: u32,
        leaf_positions: &[UVec3],
    ) -> Result<()> {
        use crate::builder::TreeLeavesInstance;

        let mut instances_data = Vec::new();

        for leaf_pos in leaf_positions.iter() {
            let voxel_pos = *leaf_pos;

            // create instance data matching GrassInstance structure
            let instance = Instance {
                pos: [voxel_pos.x, voxel_pos.y, voxel_pos.z],
                ty: 0, // not in use for now
            };

            instances_data.push(instance);
        }

        log::info!(
            "Created {} leaf instances for tree {}",
            instances_data.len(),
            tree_id
        );

        // calculate AABB based on actual leaf positions
        let scaled_leaf_positions = leaf_positions
            .iter()
            .map(|leaf| {
                Vec3::new(
                    leaf.x as f32 / 256.0,
                    leaf.y as f32 / 256.0,
                    leaf.z as f32 / 256.0,
                )
            })
            .collect::<Vec<_>>();
        let leaves_aabb = crate::builder::InstanceResources::compute_leaves_aabb(
            &scaled_leaf_positions,
            0.2, // Default margin to cover leaf radius
        );

        // create new tree leaves instance
        let mut tree_leaves_instance = TreeLeavesInstance::new(
            tree_id,
            leaves_aabb,
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
        );

        // fill with instance data if we have any
        if !instances_data.is_empty() {
            tree_leaves_instance
                .resources
                .instances_buf
                .fill(&instances_data)?;
            tree_leaves_instance.resources.instances_len = instances_data.len() as u32;
        } else {
            tree_leaves_instance.resources.instances_len = 0;
        }

        // add/update the tree instance in HashMap
        surface_resources
            .instances
            .leaves_instances
            .insert(tree_id, tree_leaves_instance);

        log::info!(
            "Added/updated tree {} with {} leaves",
            tree_id,
            instances_data.len()
        );
        Ok(())
    }

    pub fn remove_tree_leaves(
        &mut self,
        surface_resources: &mut SurfaceResources,
        tree_id: u32,
    ) -> Result<()> {
        if let Some(removed_instance) = surface_resources
            .instances
            .leaves_instances
            .remove(&tree_id)
        {
            log::info!(
                "Removed tree {} with {} leaves",
                tree_id,
                removed_instance.resources.instances_len
            );
        } else {
            log::warn!("Attempted to remove non-existent tree {}", tree_id);
        }
        Ok(())
    }

    #[allow(dead_code)]
    pub fn update_tree_leaves(
        &mut self,
        surface_resources: &mut SurfaceResources,
        tree_id: u32,
        leaf_positions: &[UVec3],
    ) -> Result<()> {
        // simply use add_tree_leaves which will overwrite existing entry
        self.add_tree_leaves(surface_resources, tree_id, leaf_positions)
    }

    #[allow(dead_code)]
    pub fn clear_all_tree_leaves(
        &mut self,
        surface_resources: &mut SurfaceResources,
    ) -> Result<()> {
        let count = surface_resources.instances.leaves_instances.len();
        surface_resources.instances.leaves_instances.clear();
        log::info!("Cleared all {} tree instances", count);
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
        self.resources.leaves_resources = LeavesResources::new_with_params(
            device.clone(),
            self.allocator.clone(),
            inner_density,
            outer_density,
            inner_radius,
            outer_radius,
            false,
        );

        self.resources.leaves_resources_lod = LeavesResources::new_with_params(
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

    pub fn query_terrain_height(&mut self, pos_xz: Vec2) -> Result<f32> {
        let heights = self.query_terrain_heights_batch(&[pos_xz])?;
        Ok(heights[0])
    }

    pub fn query_terrain_heights_batch(&mut self, positions: &[Vec2]) -> Result<Vec<f32>> {
        let query_count = positions.len() as u32;
        if query_count == 0 {
            return Ok(vec![]);
        }

        // update query count
        let count_data = StructMemberDataBuilder::from_buffer(&self.resources.terrain_query_count)
            .set_field(
                "valid_query_count",
                PlainMemberTypeWithData::UInt(query_count),
            )
            .build()?;
        self.resources
            .terrain_query_count
            .fill_with_raw_u8(&count_data)?;

        // update query positions
        let mut position_data = Vec::with_capacity(positions.len() * 2);
        for pos in positions {
            position_data.push(pos.x);
            position_data.push(pos.y);
        }
        self.resources.terrain_query_info.fill(&position_data)?;

        execute_one_time_command(
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

        // read back results
        let raw_data = self.resources.terrain_query_result.read_back().unwrap();
        let height_data: &[f32] = unsafe {
            std::slice::from_raw_parts(raw_data.as_ptr() as *const f32, query_count as usize)
        };
        Ok(height_data.to_vec())
    }
}
