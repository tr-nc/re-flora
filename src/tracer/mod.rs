mod resources;
pub use resources::*;

mod denoiser_resources;
pub use denoiser_resources::*;

mod extent_dependent_resources;
pub use extent_dependent_resources::*;

mod vertex;
pub use vertex::*;

mod voxel_encoding;

mod voxel_geometry;

mod grass_construct;

mod leaves_construct;

use glam::{Mat4, UVec3, Vec2, Vec3};
use winit::event::KeyEvent;

use crate::audio::AudioEngine;
use crate::builder::{ContreeBuilderResources, SceneAccelBuilderResources, SurfaceResources};
use crate::gameplay::{calculate_directional_light_matrices, Camera, CameraDesc};
use crate::geom::UAabb3;
use crate::resource::ResourceContainer;
use crate::util::{ShaderCompiler, TimeInfo};
use crate::vkn::{
    execute_one_time_command, Allocator, AttachmentDescOuter, AttachmentType, Buffer,
    CommandBuffer, ComputePipeline, DescriptorPool, Extent2D, Extent3D, Framebuffer,
    GraphicsPipeline, GraphicsPipelineDesc, MemoryBarrier, PipelineBarrier,
    PlainMemberTypeWithData, RenderPass, ShaderModule, StructMemberDataBuilder,
    StructMemberDataReader, Texture, Viewport, VulkanContext,
};
use anyhow::Result;
use ash::vk;

pub struct TracerDesc {
    pub scaling_factor: f32,
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

    tracer_ppl: ComputePipeline,
    temporal_ppl: ComputePipeline,
    spatial_ppl: ComputePipeline,
    composition_ppl: ComputePipeline,
    taa_ppl: ComputePipeline,
    post_processing_ppl: ComputePipeline,
    player_collider_ppl: ComputePipeline,
    terrain_query_ppl: ComputePipeline,
    tracer_shadow_ppl: ComputePipeline,
    vsm_creation_ppl: ComputePipeline,
    vsm_blur_h_ppl: ComputePipeline,
    vsm_blur_v_ppl: ComputePipeline,
    god_ray_ppl: ComputePipeline,

    grass_ppl: GraphicsPipeline,
    grass_render_pass: RenderPass,
    color_depth_framebuffer: Framebuffer,

    leaves_ppl: GraphicsPipeline,
    leaves_render_pass: RenderPass,

    leaves_shadow_ppl: GraphicsPipeline,
    leaves_shadow_render_pass: RenderPass,
    leaves_shadow_framebuffer: Framebuffer,

    #[allow(dead_code)]
    pool: DescriptorPool,
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

        let tracer_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/tracer.comp",
            "main",
        )
        .unwrap();

        let tracer_shadow_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/tracer_shadow.comp",
            "main",
        )
        .unwrap();

        let vsm_creation_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/vsm_creation.comp",
            "main",
        )
        .unwrap();

        let vsm_blur_h_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/vsm_blur_h.comp",
            "main",
        )
        .unwrap();

        let vsm_blur_v_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/vsm_blur_v.comp",
            "main",
        )
        .unwrap();

        let god_ray_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/god_ray.comp",
            "main",
        )
        .unwrap();

        let temporal_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/denoiser/temporal.comp",
            "main",
        )
        .unwrap();

        let spatial_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/denoiser/spatial.comp",
            "main",
        )
        .unwrap();

        let composition_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/composition.comp",
            "main",
        )
        .unwrap();

        let taa_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/taa.comp",
            "main",
        )
        .unwrap();

        let post_processing_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/post_processing.comp",
            "main",
        )
        .unwrap();

        let player_collider_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/player_collider.comp",
            "main",
        )
        .unwrap();

        let terrain_query_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            &shader_compiler,
            "shader/tracer/terrain_query.comp",
            "main",
        )
        .unwrap();

        let grass_vert_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/grass.vert",
            "main",
        )
        .unwrap();
        let grass_frag_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/grass.frag",
            "main",
        )
        .unwrap();

        let leaves_vert_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/leaves.vert",
            "main",
        )
        .unwrap();
        let leaves_frag_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/leaves.frag",
            "main",
        )
        .unwrap();

        let leaves_shadow_vert_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/leaves_shadow.vert",
            "main",
        )
        .unwrap();
        let leaves_shadow_frag_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/leaves_shadow.frag",
            "main",
        )
        .unwrap();

        let resources = TracerResources::new(
            &vulkan_ctx,
            allocator.clone(),
            &grass_vert_sm,
            &leaves_vert_sm,
            &tracer_sm,
            &tracer_shadow_sm,
            &composition_sm,
            &temporal_sm,
            &spatial_sm,
            &taa_sm,
            &god_ray_sm,
            &post_processing_sm,
            &player_collider_sm,
            &terrain_query_sm,
            render_extent,
            screen_extent,
            Extent2D::new(1024, 1024),
            1000, // max_terrain_queries
        );

        let device = vulkan_ctx.device();

        let tracer_ppl = ComputePipeline::new(device, &tracer_sm);
        let tracer_shadow_ppl = ComputePipeline::new(device, &tracer_shadow_sm);
        let vsm_creation_ppl = ComputePipeline::new(device, &vsm_creation_sm);
        let vsm_blur_h_ppl = ComputePipeline::new(device, &vsm_blur_h_sm);
        let vsm_blur_v_ppl = ComputePipeline::new(device, &vsm_blur_v_sm);
        let god_ray_ppl = ComputePipeline::new(device, &god_ray_sm);
        let temporal_ppl = ComputePipeline::new(device, &temporal_sm);
        let spatial_ppl = ComputePipeline::new(device, &spatial_sm);
        let composition_ppl = ComputePipeline::new(device, &composition_sm);
        let taa_ppl = ComputePipeline::new(device, &taa_sm);
        let player_collider_ppl = ComputePipeline::new(device, &player_collider_sm);
        let terrain_query_ppl = ComputePipeline::new(device, &terrain_query_sm);
        let post_processing_ppl = ComputePipeline::new(device, &post_processing_sm);

        tracer_ppl
            .auto_create_descriptor_sets(
                &pool,
                &[&resources, contree_builder_resources, scene_accel_resources],
            )
            .unwrap();
        tracer_shadow_ppl
            .auto_create_descriptor_sets(
                &pool,
                &[&resources, contree_builder_resources, scene_accel_resources],
            )
            .unwrap();
        vsm_creation_ppl
            .auto_create_descriptor_sets(&pool, &[&resources])
            .unwrap();
        vsm_blur_h_ppl
            .auto_create_descriptor_sets(&pool, &[&resources])
            .unwrap();
        vsm_blur_v_ppl
            .auto_create_descriptor_sets(&pool, &[&resources])
            .unwrap();
        god_ray_ppl
            .auto_create_descriptor_sets(&pool, &[&resources])
            .unwrap();
        temporal_ppl
            .auto_create_descriptor_sets(&pool, &[&resources])
            .unwrap();
        spatial_ppl
            .auto_create_descriptor_sets(&pool, &[&resources])
            .unwrap();
        composition_ppl
            .auto_create_descriptor_sets(&pool, &[&resources])
            .unwrap();
        taa_ppl
            .auto_create_descriptor_sets(&pool, &[&resources])
            .unwrap();
        player_collider_ppl
            .auto_create_descriptor_sets(
                &pool,
                &[&resources, contree_builder_resources, scene_accel_resources],
            )
            .unwrap();
        terrain_query_ppl
            .auto_create_descriptor_sets(
                &pool,
                &[&resources, contree_builder_resources, scene_accel_resources],
            )
            .unwrap();
        post_processing_ppl
            .auto_create_descriptor_sets(&pool, &[&resources])
            .unwrap();

        let (grass_ppl, grass_render_pass) = Self::create_grass_render_pass_and_graphics_pipeline(
            &vulkan_ctx,
            &grass_vert_sm,
            &grass_frag_sm,
            resources.extent_dependent_resources.gfx_output_tex.clone(),
            resources.extent_dependent_resources.gfx_depth_tex.clone(),
        );

        let (leaves_ppl, leaves_render_pass) =
            Self::create_leaves_render_pass_and_graphics_pipeline(
                &vulkan_ctx,
                &leaves_vert_sm,
                &leaves_frag_sm,
                resources.extent_dependent_resources.gfx_output_tex.clone(),
                resources.extent_dependent_resources.gfx_depth_tex.clone(),
            );

        let (leaves_shadow_ppl, leaves_shadow_render_pass) =
            Self::create_leaves_shadow_render_pass_and_graphics_pipeline(
                &vulkan_ctx,
                &leaves_shadow_vert_sm,
                &leaves_shadow_frag_sm,
                resources.shadow_map_tex.clone(),
            );

        grass_ppl
            .auto_create_descriptor_sets(&pool, &[&resources])
            .unwrap();
        leaves_ppl
            .auto_create_descriptor_sets(&pool, &[&resources])
            .unwrap();
        leaves_shadow_ppl
            .auto_create_descriptor_sets(&pool, &[&resources])
            .unwrap();

        let color_depth_framebuffer = Self::create_main_framebuffer(
            &vulkan_ctx,
            &grass_render_pass,
            &resources.extent_dependent_resources.gfx_output_tex,
            &resources.extent_dependent_resources.gfx_depth_tex,
        );
        let leaves_shadow_framebuffer = Self::create_shadow_map_framebuffer(
            &vulkan_ctx,
            &leaves_shadow_render_pass,
            &resources.shadow_map_tex,
        );

        Ok(Self {
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
            tracer_ppl,
            tracer_shadow_ppl,
            god_ray_ppl,
            temporal_ppl,
            spatial_ppl,
            composition_ppl,
            taa_ppl,
            post_processing_ppl,
            player_collider_ppl,
            terrain_query_ppl,
            vsm_creation_ppl,
            vsm_blur_h_ppl,
            vsm_blur_v_ppl,
            grass_ppl,
            grass_render_pass,
            color_depth_framebuffer,
            leaves_ppl,
            leaves_render_pass,
            leaves_shadow_ppl,
            leaves_shadow_render_pass,
            leaves_shadow_framebuffer,
            pool,
        })
    }

    fn create_grass_render_pass_and_graphics_pipeline(
        vulkan_ctx: &VulkanContext,
        vert_sm: &ShaderModule,
        frag_sm: &ShaderModule,
        output_tex: Texture,
        depth_tex: Texture,
    ) -> (GraphicsPipeline, RenderPass) {
        let render_pass = {
            RenderPass::with_attachments(
                vulkan_ctx.device().clone(),
                &[
                    AttachmentDescOuter {
                        texture: output_tex,
                        load_op: vk::AttachmentLoadOp::CLEAR,
                        store_op: vk::AttachmentStoreOp::STORE,
                        initial_layout: vk::ImageLayout::UNDEFINED,
                        final_layout: vk::ImageLayout::GENERAL,
                        ty: AttachmentType::Color,
                    },
                    AttachmentDescOuter {
                        texture: depth_tex,
                        load_op: vk::AttachmentLoadOp::CLEAR,
                        store_op: vk::AttachmentStoreOp::STORE,
                        initial_layout: vk::ImageLayout::UNDEFINED,
                        final_layout: vk::ImageLayout::GENERAL,
                        ty: AttachmentType::Depth,
                    },
                ],
            )
        };

        let gfx_ppl = GraphicsPipeline::new(
            vulkan_ctx.device(),
            &vert_sm,
            &frag_sm,
            &render_pass,
            &GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
            Some(1),
        );

        (gfx_ppl, render_pass)
    }

    fn create_leaves_render_pass_and_graphics_pipeline(
        vulkan_ctx: &VulkanContext,
        vert_sm: &ShaderModule,
        frag_sm: &ShaderModule,
        output_tex: Texture,
        depth_tex: Texture,
    ) -> (GraphicsPipeline, RenderPass) {
        let render_pass = {
            RenderPass::with_attachments(
                vulkan_ctx.device().clone(),
                &[
                    AttachmentDescOuter {
                        texture: output_tex,
                        load_op: vk::AttachmentLoadOp::LOAD,
                        store_op: vk::AttachmentStoreOp::STORE,
                        initial_layout: vk::ImageLayout::GENERAL,
                        final_layout: vk::ImageLayout::GENERAL,
                        ty: AttachmentType::Color,
                    },
                    AttachmentDescOuter {
                        texture: depth_tex,
                        load_op: vk::AttachmentLoadOp::LOAD,
                        store_op: vk::AttachmentStoreOp::STORE,
                        initial_layout: vk::ImageLayout::GENERAL,
                        final_layout: vk::ImageLayout::GENERAL,
                        ty: AttachmentType::Depth,
                    },
                ],
            )
        };

        let gfx_ppl = GraphicsPipeline::new(
            vulkan_ctx.device(),
            &vert_sm,
            &frag_sm,
            &render_pass,
            &GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
            Some(1),
        );

        (gfx_ppl, render_pass)
    }

    fn create_leaves_shadow_render_pass_and_graphics_pipeline(
        vulkan_ctx: &VulkanContext,
        vert_sm: &ShaderModule,
        frag_sm: &ShaderModule,
        shadow_map_tex: Texture,
    ) -> (GraphicsPipeline, RenderPass) {
        let render_pass = {
            RenderPass::with_attachments(
                vulkan_ctx.device().clone(),
                &[AttachmentDescOuter {
                    texture: shadow_map_tex,
                    load_op: vk::AttachmentLoadOp::CLEAR,
                    store_op: vk::AttachmentStoreOp::STORE,
                    initial_layout: vk::ImageLayout::UNDEFINED,
                    final_layout: vk::ImageLayout::GENERAL,
                    ty: AttachmentType::Depth,
                }],
            )
        };
        let gfx_ppl = GraphicsPipeline::new(
            vulkan_ctx.device(),
            &vert_sm,
            &frag_sm,
            &render_pass,
            &GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
            Some(1),
        );
        (gfx_ppl, render_pass)
    }

    /// A framebuffer that contains the color and depth textures for the main render pass
    fn create_main_framebuffer(
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
    fn create_shadow_map_framebuffer(
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

        self.color_depth_framebuffer = Self::create_main_framebuffer(
            &self.vulkan_ctx,
            &self.grass_render_pass,
            &self.resources.extent_dependent_resources.gfx_output_tex,
            &self.resources.extent_dependent_resources.gfx_depth_tex,
        );

        self.update_sets(contree_builder_resources, scene_accel_resources);
    }

    fn update_sets(
        &mut self,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
    ) {
        let update_fn = |ppl: &ComputePipeline, resources: &[&dyn ResourceContainer]| {
            ppl.auto_update_descriptor_sets(resources).unwrap()
        };

        // Pipelines that need all resources (tracer, scene_accel, contree)
        let all_resources = &[
            &self.resources as &dyn ResourceContainer,
            contree_builder_resources as &dyn ResourceContainer,
            scene_accel_resources as &dyn ResourceContainer,
        ];
        update_fn(&self.tracer_ppl, all_resources);
        update_fn(&self.tracer_shadow_ppl, all_resources);
        update_fn(&self.player_collider_ppl, all_resources);
        update_fn(&self.terrain_query_ppl, all_resources);

        // Pipelines that only need tracer resources
        let tracer_resources = &[&self.resources as &dyn ResourceContainer];
        update_fn(&self.vsm_creation_ppl, tracer_resources);
        update_fn(&self.vsm_blur_h_ppl, tracer_resources);
        update_fn(&self.vsm_blur_v_ppl, tracer_resources);
        update_fn(&self.god_ray_ppl, tracer_resources);
        update_fn(&self.temporal_ppl, tracer_resources);
        update_fn(&self.spatial_ppl, tracer_resources);
        update_fn(&self.composition_ppl, tracer_resources);
        update_fn(&self.taa_ppl, tracer_resources);
        update_fn(&self.post_processing_ppl, tracer_resources);
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
        is_spatial_denoising_skipped: bool,
        is_taa_enabled: bool,
        god_ray_max_depth: f32,
        god_ray_max_checks: u32,
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
        grass_bottom_color: Vec3,
        grass_tip_color: Vec3,
        leaf_bottom_color: Vec3,
        leaf_tip_color: Vec3,
    ) -> Result<()> {
        // camera info
        let view_mat = self.camera.get_view_mat();
        let proj_mat = self.camera.get_proj_mat();
        self.current_view_proj_mat = proj_mat * view_mat;
        update_cam_info(&mut self.resources.camera_info, view_mat, proj_mat)?;

        // shadow cam info
        let world_bound = self.chunk_bound.into();
        let (shadow_view_mat, shadow_proj_mat) =
            calculate_directional_light_matrices(world_bound, sun_dir);
        self.current_shadow_view_proj_mat = shadow_proj_mat * shadow_view_mat;
        update_cam_info(
            &mut self.resources.shadow_camera_info,
            shadow_view_mat,
            shadow_proj_mat,
        )?;

        // camera info prev frame
        update_cam_info(
            &mut self.resources.camera_info_prev_frame,
            self.camera_view_mat_prev_frame,
            self.camera_proj_mat_prev_frame,
        )?;

        update_taa_info(&self.resources, is_taa_enabled)?;

        update_god_ray_info(&self.resources, god_ray_max_depth, god_ray_max_checks)?;

        update_post_processing_info(&self.resources, self.desc.scaling_factor)?;

        update_player_collider_info(&self.resources, self.camera.position(), self.camera.front())?;

        update_grass_info(
            &self.resources,
            time_info.time_since_start(),
            grass_bottom_color,
            grass_tip_color,
        )?;

        update_leaf_info(
            &self.resources,
            time_info.time_since_start(),
            leaf_bottom_color,
            leaf_tip_color,
        )?;

        update_gui_input(&self.resources, debug_float, debug_bool, debug_uint)?;

        update_sun_info(
            &self.resources,
            sun_dir,
            sun_size,
            sun_color,
            sun_luminance,
            sun_altitude,
            sun_azimuth,
        )?;

        update_shading_info(&self.resources, ambient_light)?;

        update_starlight_info(
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

        update_env_info(&self.resources, time_info.total_frame_count() as u32)?;

        update_denoiser_info(
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
            is_spatial_denoising_skipped,
        )?;

        self.camera_view_mat_prev_frame = self.camera.get_view_mat();
        self.camera_proj_mat_prev_frame = self.camera.get_proj_mat();

        return Ok(());

        fn update_denoiser_info(
            temporal_info: &mut Buffer,
            spatial_info: &mut Buffer,
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
        ) -> Result<()> {
            update_temporal_info(temporal_info, temporal_position_phi, temporal_alpha)?;
            update_spatial_info(
                spatial_info,
                phi_c,
                phi_n,
                phi_p,
                min_phi_z,
                max_phi_z,
                phi_z_stable_sample_count,
                is_changing_lum_phi,
                is_spatial_denoising_skipped,
            )?;
            return Ok(());

            fn update_temporal_info(
                temporal_info: &mut Buffer,
                temporal_position_phi: f32,
                temporal_alpha: f32,
            ) -> Result<()> {
                let data = StructMemberDataBuilder::from_buffer(temporal_info)
                    .set_field(
                        "temporal_position_phi",
                        PlainMemberTypeWithData::Float(temporal_position_phi),
                    )
                    .set_field(
                        "temporal_alpha",
                        PlainMemberTypeWithData::Float(temporal_alpha),
                    )
                    .build()?;
                temporal_info.fill_with_raw_u8(&data)?;
                Ok(())
            }

            fn update_spatial_info(
                spatial_info: &mut Buffer,
                phi_c: f32,
                phi_n: f32,
                phi_p: f32,
                min_phi_z: f32,
                max_phi_z: f32,
                phi_z_stable_sample_count: f32,
                is_changing_lum_phi: bool,
                is_spatial_denoising_skipped: bool,
            ) -> Result<()> {
                let data = StructMemberDataBuilder::from_buffer(spatial_info)
                    .set_field("phi_c", PlainMemberTypeWithData::Float(phi_c))
                    .set_field("phi_n", PlainMemberTypeWithData::Float(phi_n))
                    .set_field("phi_p", PlainMemberTypeWithData::Float(phi_p))
                    .set_field("min_phi_z", PlainMemberTypeWithData::Float(min_phi_z))
                    .set_field("max_phi_z", PlainMemberTypeWithData::Float(max_phi_z))
                    .set_field(
                        "phi_z_stable_sample_count",
                        PlainMemberTypeWithData::Float(phi_z_stable_sample_count),
                    )
                    .set_field(
                        "is_changing_lum_phi",
                        PlainMemberTypeWithData::UInt(is_changing_lum_phi as u32),
                    )
                    .set_field(
                        "is_spatial_denoising_skipped",
                        PlainMemberTypeWithData::UInt(is_spatial_denoising_skipped as u32),
                    )
                    .build()?;
                spatial_info.fill_with_raw_u8(&data)?;
                Ok(())
            }
        }

        fn update_gui_input(
            resources: &TracerResources,
            debug_float: f32,
            debug_bool: bool,
            debug_uint: u32,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.gui_input)
                .set_field("debug_float", PlainMemberTypeWithData::Float(debug_float))
                .set_field(
                    "debug_bool",
                    PlainMemberTypeWithData::UInt(debug_bool as u32),
                )
                .set_field("debug_uint", PlainMemberTypeWithData::UInt(debug_uint))
                .build()?;
            resources.gui_input.fill_with_raw_u8(&data)?;
            return Ok(());
        }

        fn update_sun_info(
            resources: &TracerResources,
            sun_dir: Vec3,
            sun_size: f32,
            sun_color: Vec3,
            sun_luminance: f32,
            sun_altitude: f32,
            sun_azimuth: f32,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.sun_info)
                .set_field("sun_dir", PlainMemberTypeWithData::Vec3(sun_dir.to_array()))
                .set_field("sun_size", PlainMemberTypeWithData::Float(sun_size))
                .set_field(
                    "sun_color",
                    PlainMemberTypeWithData::Vec3(sun_color.to_array()),
                )
                .set_field(
                    "sun_luminance",
                    PlainMemberTypeWithData::Float(sun_luminance),
                )
                .set_field("sun_altitude", PlainMemberTypeWithData::Float(sun_altitude))
                .set_field("sun_azimuth", PlainMemberTypeWithData::Float(sun_azimuth))
                .build()?;
            resources.sun_info.fill_with_raw_u8(&data)?;
            return Ok(());
        }

        fn update_shading_info(resources: &TracerResources, ambient_light: Vec3) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.shading_info)
                .set_field(
                    "ambient_light",
                    PlainMemberTypeWithData::Vec3(ambient_light.to_array()),
                )
                .build()?;
            resources.shading_info.fill_with_raw_u8(&data)?;
            return Ok(());
        }

        fn update_starlight_info(
            resources: &TracerResources,
            iterations: i32,
            formuparam: f32,
            volsteps: i32,
            stepsize: f32,
            zoom: f32,
            tile: f32,
            speed: f32,
            brightness: f32,
            darkmatter: f32,
            distfading: f32,
            saturation: f32,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.starlight_info)
                .set_field("iterations", PlainMemberTypeWithData::Int(iterations))
                .set_field("formuparam", PlainMemberTypeWithData::Float(formuparam))
                .set_field("volsteps", PlainMemberTypeWithData::Int(volsteps))
                .set_field("stepsize", PlainMemberTypeWithData::Float(stepsize))
                .set_field("zoom", PlainMemberTypeWithData::Float(zoom))
                .set_field("tile", PlainMemberTypeWithData::Float(tile))
                .set_field("speed", PlainMemberTypeWithData::Float(speed))
                .set_field("brightness", PlainMemberTypeWithData::Float(brightness))
                .set_field("darkmatter", PlainMemberTypeWithData::Float(darkmatter))
                .set_field("distfading", PlainMemberTypeWithData::Float(distfading))
                .set_field("saturation", PlainMemberTypeWithData::Float(saturation))
                .build()?;
            resources.starlight_info.fill_with_raw_u8(&data)?;
            return Ok(());
        }

        fn update_cam_info(camera_info: &mut Buffer, view_mat: Mat4, proj_mat: Mat4) -> Result<()> {
            let view_proj_mat = proj_mat * view_mat;

            let camera_pos = view_mat.inverse().w_axis;
            let data = StructMemberDataBuilder::from_buffer(camera_info)
                .set_field("pos", PlainMemberTypeWithData::Vec4(camera_pos.to_array()))
                .set_field(
                    "view_mat",
                    PlainMemberTypeWithData::Mat4(view_mat.to_cols_array_2d()),
                )
                .set_field(
                    "view_mat_inv",
                    PlainMemberTypeWithData::Mat4(view_mat.inverse().to_cols_array_2d()),
                )
                .set_field(
                    "proj_mat",
                    PlainMemberTypeWithData::Mat4(proj_mat.to_cols_array_2d()),
                )
                .set_field(
                    "proj_mat_inv",
                    PlainMemberTypeWithData::Mat4(proj_mat.inverse().to_cols_array_2d()),
                )
                .set_field(
                    "view_proj_mat",
                    PlainMemberTypeWithData::Mat4(view_proj_mat.to_cols_array_2d()),
                )
                .set_field(
                    "view_proj_mat_inv",
                    PlainMemberTypeWithData::Mat4(view_proj_mat.inverse().to_cols_array_2d()),
                )
                .build()?;
            camera_info.fill_with_raw_u8(&data)?;
            Ok(())
        }

        fn update_env_info(resources: &TracerResources, frame_serial_idx: u32) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.env_info)
                .set_field(
                    "frame_serial_idx",
                    PlainMemberTypeWithData::UInt(frame_serial_idx),
                )
                .build()?;
            resources.env_info.fill_with_raw_u8(&data)?;
            Ok(())
        }

        fn update_grass_info(
            resources: &TracerResources,
            time: f32,
            bottom_color: Vec3,
            tip_color: Vec3,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.grass_info)
                .set_field("time", PlainMemberTypeWithData::Float(time))
                .set_field(
                    "bottom_color",
                    PlainMemberTypeWithData::Vec3(bottom_color.to_array()),
                )
                .set_field(
                    "tip_color",
                    PlainMemberTypeWithData::Vec3(tip_color.to_array()),
                )
                .build()?;
            resources.grass_info.fill_with_raw_u8(&data)?;
            Ok(())
        }

        fn update_leaf_info(
            resources: &TracerResources,
            time: f32,
            bottom_color: Vec3,
            tip_color: Vec3,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.leaf_info)
                .set_field("time", PlainMemberTypeWithData::Float(time))
                .set_field(
                    "bottom_color",
                    PlainMemberTypeWithData::Vec3(bottom_color.to_array()),
                )
                .set_field(
                    "tip_color",
                    PlainMemberTypeWithData::Vec3(tip_color.to_array()),
                )
                .build()?;
            resources.leaf_info.fill_with_raw_u8(&data)?;
            Ok(())
        }

        fn update_taa_info(resources: &TracerResources, is_taa_enabled: bool) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.taa_info)
                .set_field(
                    "is_taa_enabled",
                    PlainMemberTypeWithData::UInt(is_taa_enabled as u32),
                )
                .build()?;
            resources.taa_info.fill_with_raw_u8(&data)?;
            Ok(())
        }

        fn update_god_ray_info(
            resources: &TracerResources,
            max_depth: f32,
            max_checks: u32,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.god_ray_info)
                .set_field("max_depth", PlainMemberTypeWithData::Float(max_depth))
                .set_field("max_checks", PlainMemberTypeWithData::UInt(max_checks))
                .build()?;
            resources.god_ray_info.fill_with_raw_u8(&data)?;
            Ok(())
        }

        fn update_post_processing_info(
            resources: &TracerResources,
            scaling_factor: f32,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.post_processing_info)
                .set_field(
                    "scaling_factor",
                    PlainMemberTypeWithData::Float(scaling_factor),
                )
                .build()?;
            resources.post_processing_info.fill_with_raw_u8(&data)?;
            Ok(())
        }

        fn update_player_collider_info(
            resources: &TracerResources,
            player_pos: Vec3,
            camera_front: Vec3,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.player_collider_info)
                .set_field(
                    "player_pos",
                    PlainMemberTypeWithData::Vec3(player_pos.to_array()),
                )
                .set_field(
                    "camera_front",
                    PlainMemberTypeWithData::Vec3(camera_front.to_array()),
                )
                .build()?;
            resources.player_collider_info.fill_with_raw_u8(&data)?;
            Ok(())
        }
    }

    pub fn record_trace(
        &mut self,
        cmdbuf: &CommandBuffer,
        surface_resources: &SurfaceResources,
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

        self.record_leaves_shadow_pass(cmdbuf, surface_resources);
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

        self.record_grass_pass(cmdbuf, surface_resources);
        frag_to_vert_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
        self.record_leaves_pass(cmdbuf, surface_resources);
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

        self.record_denoiser_pass(&cmdbuf);

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

    fn record_grass_pass(&self, cmdbuf: &CommandBuffer, surface_resources: &SurfaceResources) {
        self.grass_ppl.record_bind(cmdbuf);

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

        self.grass_render_pass
            .record_begin(cmdbuf, &self.color_depth_framebuffer, &clear_values);

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

        // must be done before record draw, can be swapped with record_viewport_scissor
        // self.grass_ppl
        //     .record_bind_descriptor_sets(cmdbuf, &self.grass_sets, 0);

        self.grass_ppl
            .record_viewport_scissor(cmdbuf, viewport, scissor);

        unsafe {
            // bind the index buffer
            self.vulkan_ctx.device().cmd_bind_index_buffer(
                cmdbuf.as_raw(),
                self.resources.grass_blade_resources.indices.as_raw(),
                0,
                vk::IndexType::UINT32, // Use 32-bit indices
            );
        }

        // now, iterate over each chunk and issue a draw call for it.
        for (aabb, instances) in &surface_resources.instances.chunk_grass_instances {
            // only draw if this chunk actually has grass instances.
            if instances.grass_instances_len == 0 {
                continue;
            }

            // perform frustum culling
            if !aabb.is_inside_frustum(self.current_view_proj_mat) {
                continue;
            }

            // bind the vertex buffers for this specific chunk.
            // binding point 0: common grass blade vertices.
            // binding point 1: per-chunk, per-instance data.
            unsafe {
                self.vulkan_ctx.device().cmd_bind_vertex_buffers(
                    cmdbuf.as_raw(),
                    0, // firstBinding
                    &[
                        self.resources.grass_blade_resources.vertices.as_raw(),
                        instances.grass_instances.as_raw(),
                    ],
                    &[0, 0], // offsets
                );
            }

            // issue the draw call for the current chunk.
            // No barriers are needed here.
            self.grass_ppl.record_indexed(
                cmdbuf,
                self.resources.grass_blade_resources.indices_len,
                instances.grass_instances_len,
                0, // firstIndex
                0, // vertexOffset
                0, // firstInstance
            );
        }

        self.grass_render_pass.record_end(cmdbuf);

        let desc = self.grass_render_pass.get_desc();
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

    fn record_leaves_pass(&self, cmdbuf: &CommandBuffer, surface_resources: &SurfaceResources) {
        // Skip rendering entirely if no leaf instances exist
        if surface_resources.instances.leaves_instances.is_empty() {
            return;
        }

        self.leaves_ppl.record_bind(cmdbuf);

        // Don't clear - we want to preserve the grass that was already rendered
        // Only clear the depth buffer to ensure proper depth testing
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

        self.leaves_render_pass
            .record_begin(cmdbuf, &self.color_depth_framebuffer, &clear_values);

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

        self.leaves_ppl
            .record_viewport_scissor(cmdbuf, viewport, scissor);

        unsafe {
            // bind the index buffer for leaves
            self.vulkan_ctx.device().cmd_bind_index_buffer(
                cmdbuf.as_raw(),
                self.resources.leaves_resources.indices.as_raw(),
                0,
                vk::IndexType::UINT32,
            );
        }

        // Loop through all tree leaves instances
        for (_tree_id, tree_instance) in &surface_resources.instances.leaves_instances {
            if tree_instance.resources.leaves_instances_len == 0 {
                continue;
            }

            // perform frustum culling
            if !tree_instance
                .aabb
                .is_inside_frustum(self.current_view_proj_mat)
            {
                continue;
            }

            // Bind vertex buffers for this instance
            unsafe {
                self.vulkan_ctx.device().cmd_bind_vertex_buffers(
                    cmdbuf.as_raw(),
                    0,
                    &[
                        self.resources.leaves_resources.vertices.as_raw(),
                        tree_instance.resources.leaves_instances.as_raw(),
                    ],
                    &[0, 0],
                );
            }

            // Render this instance
            self.leaves_ppl.record_indexed(
                cmdbuf,
                self.resources.leaves_resources.indices_len,
                tree_instance.resources.leaves_instances_len,
                0,
                0,
                0,
            );
        }

        self.leaves_render_pass.record_end(cmdbuf);

        let desc = self.leaves_render_pass.get_desc();
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

    fn record_leaves_shadow_pass(
        &self,
        cmdbuf: &CommandBuffer,
        surface_resources: &SurfaceResources,
    ) {
        self.leaves_shadow_ppl.record_bind(cmdbuf);

        let clear_values = [vk::ClearValue {
            depth_stencil: vk::ClearDepthStencilValue {
                depth: 1.0,
                stencil: 0,
            },
        }];

        self.leaves_shadow_render_pass.record_begin(
            cmdbuf,
            &self.leaves_shadow_framebuffer,
            &clear_values,
        );

        let shadow_extent = self.resources.shadow_map_tex.get_image().get_desc().extent;
        let viewport = Viewport::from_extent(shadow_extent.as_extent_2d().unwrap());
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D {
                width: shadow_extent.width,
                height: shadow_extent.height,
            },
        };

        self.leaves_shadow_ppl
            .record_viewport_scissor(cmdbuf, viewport, scissor);

        unsafe {
            self.vulkan_ctx.device().cmd_bind_index_buffer(
                cmdbuf.as_raw(),
                self.resources.leaves_resources.indices.as_raw(),
                0,
                vk::IndexType::UINT32,
            );
        }

        // Loop through all tree leaves instances
        for (_tree_id, tree_instance) in &surface_resources.instances.leaves_instances {
            if tree_instance.resources.leaves_instances_len == 0 {
                continue;
            }

            // perform frustum culling using shadow camera
            if !tree_instance
                .aabb
                .is_inside_frustum(self.current_shadow_view_proj_mat)
            {
                continue;
            }

            unsafe {
                self.vulkan_ctx.device().cmd_bind_vertex_buffers(
                    cmdbuf.as_raw(),
                    0,
                    &[
                        self.resources.leaves_resources.vertices.as_raw(),
                        tree_instance.resources.leaves_instances.as_raw(),
                    ],
                    &[0, 0],
                );
            }

            // Render this instance for shadow map
            self.leaves_shadow_ppl.record_indexed(
                cmdbuf,
                self.resources.leaves_resources.indices_len,
                tree_instance.resources.leaves_instances_len,
                0,
                0,
                0,
            );
        }

        self.leaves_shadow_render_pass.record_end(cmdbuf);

        let desc = self.leaves_shadow_render_pass.get_desc();
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
        self.tracer_shadow_ppl.record(
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
        self.vsm_creation_ppl.record(cmdbuf, extent, None);

        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        self.vsm_blur_h_ppl.record(cmdbuf, extent, None);

        compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);

        self.vsm_blur_v_ppl.record(cmdbuf, extent, None);
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

        self.tracer_ppl.record(
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

        self.god_ray_ppl.record(
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

    fn record_denoiser_pass(&self, cmdbuf: &CommandBuffer) {
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

        self.temporal_ppl.record(cmdbuf, extent, None);

        const A_TROUS_ITERATION_COUNT: u32 = 3;
        for i in 0..A_TROUS_ITERATION_COUNT {
            compute_to_compute_barrier.record_insert(self.vulkan_ctx.device(), cmdbuf);
            self.spatial_ppl.record(
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
    }

    fn record_composition_pass(&self, cmdbuf: &CommandBuffer) {
        self.resources
            .extent_dependent_resources
            .composited_tex
            .get_image()
            .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);

        self.composition_ppl.record(
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

        self.taa_ppl.record(
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

        self.post_processing_ppl.record(
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
        self.player_collider_ppl
            .record(cmdbuf, Extent3D::new(1, 1, 1), None);
    }

    pub fn handle_keyboard(&mut self, key_event: &KeyEvent) {
        self.camera.handle_keyboard(key_event);
    }

    pub fn handle_mouse(&mut self, delta: Vec2) {
        self.camera.handle_mouse(delta);
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

        #[repr(C)]
        #[derive(Copy, Clone)]
        struct LeafInstance {
            position: [u32; 3],
            leaf_type: u32,
        }

        let mut instances_data = Vec::new();

        for leaf_pos in leaf_positions.iter() {
            let voxel_pos = *leaf_pos;

            // Create instance data matching GrassInstance structure
            let instance = LeafInstance {
                position: [voxel_pos.x, voxel_pos.y, voxel_pos.z],
                leaf_type: 0, // not in use for now
            };

            instances_data.push(instance);
        }

        log::info!(
            "Created {} leaf instances for tree {}",
            instances_data.len(),
            tree_id
        );

        // Calculate AABB based on actual leaf positions
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

        // Create new tree leaves instance
        let mut tree_leaves_instance = TreeLeavesInstance::new(
            tree_id,
            leaves_aabb,
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
        );

        // Fill with instance data if we have any
        if !instances_data.is_empty() {
            tree_leaves_instance
                .resources
                .leaves_instances
                .fill(&instances_data)?;
            tree_leaves_instance.resources.leaves_instances_len = instances_data.len() as u32;
        } else {
            tree_leaves_instance.resources.leaves_instances_len = 0;
        }

        // Add/update the tree instance in HashMap
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
                removed_instance.resources.leaves_instances_len
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
        // Simply use add_tree_leaves which will overwrite existing entry
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
        density_min: f32,
        density_max: f32,
        radius: f32,
    ) -> Result<()> {
        // Regenerate leaves resources with new density parameters
        let device = self.vulkan_ctx.device();

        // Create new leaves resources with the provided parameters
        // Note: We create new leaves resources with default params, then immediately overwrite
        // This approach maintains the same API while using the constructor's defaults
        self.resources.leaves_resources = LeavesResources::new_with_params(
            device.clone(),
            self.allocator.clone(),
            density_min,
            density_max,
            radius,
        );

        log::info!(
            "Regenerated leaves with density_min: {}, density_max: {}, radius: {}",
            density_min,
            density_max,
            radius
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

        // Update query count
        let count_data = StructMemberDataBuilder::from_buffer(&self.resources.terrain_query_count)
            .set_field(
                "valid_query_count",
                PlainMemberTypeWithData::UInt(query_count),
            )
            .build()?;
        self.resources
            .terrain_query_count
            .fill_with_raw_u8(&count_data)?;

        // Update query positions
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
                self.terrain_query_ppl
                    .record(cmdbuf, Extent3D::new(query_count, 1, 1), None);
            },
        );

        // Read back results
        let raw_data = self.resources.terrain_query_result.read_back().unwrap();
        let height_data: &[f32] = unsafe {
            std::slice::from_raw_parts(raw_data.as_ptr() as *const f32, query_count as usize)
        };
        Ok(height_data.to_vec())
    }
}
