mod resources;
pub use resources::*;

mod denoiser_resources;
pub use denoiser_resources::*;

mod vertex;
pub use vertex::*;

mod grass_construct;

use glam::{Mat4, Vec2, Vec3};
use winit::event::KeyEvent;

use crate::builder::SurfaceResources;
use crate::gameplay::{calculate_directional_light_matrices, Camera, CameraDesc};
use crate::geom::UAabb3;
use crate::util::{ShaderCompiler, TimeInfo};
use crate::vkn::{
    Allocator, AttachmentDesc, AttachmentReference, Buffer, ComputePipeline, DescriptorPool,
    DescriptorSet, Extent2D, Extent3D, Framebuffer, GraphicsPipeline, GraphicsPipelineDesc,
    MemoryBarrier, PipelineBarrier, PlainMemberTypeWithData, RenderPass, RenderPassDesc,
    ShaderModule, StructMemberDataBuilder, StructMemberDataReader, SubpassDesc, Texture, Viewport,
    WriteDescriptorSet,
};
use crate::vkn::{CommandBuffer, VulkanContext};
use anyhow::Result;
use ash::vk;

pub struct TracerDesc {
    pub scaling_factor: f32,
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

    tracer_ppl: ComputePipeline,
    temporal_ppl: ComputePipeline,
    spatial_ppl: ComputePipeline,
    composition_ppl: ComputePipeline,
    taa_ppl: ComputePipeline,
    post_processing_ppl: ComputePipeline,
    player_collider_ppl: ComputePipeline,

    tracer_shadow_ppl: ComputePipeline,
    #[allow(dead_code)]
    tracer_shadow_sets: [DescriptorSet; 1],

    vsm_creation_ppl: ComputePipeline,
    vsm_blur_h_ppl: ComputePipeline,
    vsm_blur_v_ppl: ComputePipeline,
    god_ray_ppl: ComputePipeline,

    grass_sets: [DescriptorSet; 1],
    grass_ppl: GraphicsPipeline,
    grass_render_pass: RenderPass,
    grass_framebuffer: Framebuffer,

    #[allow(dead_code)]
    shadow_sets: [DescriptorSet; 1],
    #[allow(dead_code)]
    shadow_ppl: GraphicsPipeline,
    #[allow(dead_code)]
    shadow_render_pass: RenderPass,
    #[allow(dead_code)]
    shadow_framebuffer: Framebuffer,

    #[allow(dead_code)]
    fixed_pool: DescriptorPool,
    #[allow(dead_code)]
    flexible_pool: DescriptorPool,

    flexible_sets: Vec<DescriptorSet>,
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
        node_data: &Buffer,
        leaf_data: &Buffer,
        scene_tex: &Texture,
        desc: TracerDesc,
    ) -> Self {
        let render_extent = Self::get_render_extent(screen_extent, desc.scaling_factor);

        let camera = Camera::new(
            Vec3::new(0.5, 1.2, 0.5),
            135.0,
            -5.0,
            CameraDesc {
                aspect_ratio: render_extent.get_aspect_ratio(),
                ..Default::default()
            },
        );

        let fixed_pool = DescriptorPool::new(vulkan_ctx.device()).unwrap();
        let flexible_pool = DescriptorPool::new(vulkan_ctx.device()).unwrap();

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

        let shadow_vert_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/shadow.vert",
            "main",
        )
        .unwrap();
        let shadow_frag_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/shadow.frag",
            "main",
        )
        .unwrap();

        let resources = TracerResources::new(
            &vulkan_ctx,
            allocator.clone(),
            &grass_vert_sm,
            &tracer_sm,
            &tracer_shadow_sm,
            &temporal_sm,
            &spatial_sm,
            &taa_sm,
            &post_processing_sm,
            &player_collider_sm,
            render_extent,
            screen_extent,
            Extent2D::new(1024, 1024),
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
        let post_processing_ppl = ComputePipeline::new(device, &post_processing_sm);

        let alloc_fixed_set_fn = |ppl: &ComputePipeline, layout_idx: u32| -> DescriptorSet {
            fixed_pool
                .allocate_set(&ppl.get_layout().get_descriptor_set_layouts()[&layout_idx])
                .unwrap()
        };
        let alloc_flexible_set_fn = |ppl: &ComputePipeline, layout_idx: u32| -> DescriptorSet {
            flexible_pool
                .allocate_set(&ppl.get_layout().get_descriptor_set_layouts()[&layout_idx])
                .unwrap()
        };

        let tracer_ds_0 = alloc_fixed_set_fn(&tracer_ppl, 0);
        let tracer_ds_1 = alloc_flexible_set_fn(&tracer_ppl, 1);
        let temporal_ds = alloc_flexible_set_fn(&temporal_ppl, 0);
        let spatial_fixed_set = alloc_fixed_set_fn(&spatial_ppl, 0);
        let spatial_flexible_set = alloc_flexible_set_fn(&spatial_ppl, 1);
        let noise_tex_ds = alloc_fixed_set_fn(&tracer_ppl, 2);
        let tracer_shadow_ds = alloc_fixed_set_fn(&tracer_shadow_ppl, 0);
        let vsm_ds_0 = alloc_fixed_set_fn(&vsm_creation_ppl, 0);
        let god_ray_ds_0 = alloc_fixed_set_fn(&god_ray_ppl, 0);
        let composition_ds = alloc_flexible_set_fn(&composition_ppl, 0);
        let taa_ds = alloc_flexible_set_fn(&taa_ppl, 0);
        let post_processing_ds = alloc_flexible_set_fn(&post_processing_ppl, 0);
        let player_collider_ds = alloc_fixed_set_fn(&player_collider_ppl, 0);
        let denoiser_ds = alloc_flexible_set_fn(&tracer_ppl, 3);

        // ignore the flexible sets
        Self::update_tracer_ds_0(&tracer_ds_0, &resources, node_data, leaf_data, scene_tex);
        Self::update_spatial_fixed_set(&spatial_fixed_set, &resources);
        Self::update_noise_tex_ds(&noise_tex_ds, &resources);
        Self::update_tracer_shadow_ds(
            &tracer_shadow_ds,
            &resources,
            node_data,
            leaf_data,
            scene_tex,
        );
        Self::update_vsm_ds_0(&vsm_ds_0, &resources);
        Self::update_player_collider_ds(
            &player_collider_ds,
            &resources,
            node_data,
            leaf_data,
            scene_tex,
        );

        god_ray_ppl.set_descriptor_sets(vec![god_ray_ds_0.clone(), noise_tex_ds.clone()]);
        temporal_ppl.set_descriptor_sets(vec![temporal_ds.clone(), denoiser_ds.clone()]);
        spatial_ppl.set_descriptor_sets(vec![
            spatial_fixed_set.clone(),
            spatial_flexible_set.clone(),
            denoiser_ds.clone(),
        ]);
        composition_ppl.set_descriptor_sets(vec![composition_ds.clone()]);
        taa_ppl.set_descriptor_sets(vec![taa_ds.clone()]);
        post_processing_ppl.set_descriptor_sets(vec![post_processing_ds.clone()]);
        player_collider_ppl.set_descriptor_sets(vec![player_collider_ds.clone()]);
        tracer_ppl.set_descriptor_sets(vec![
            tracer_ds_0.clone(),
            tracer_ds_1.clone(),
            noise_tex_ds.clone(),
            denoiser_ds.clone(),
        ]);
        tracer_shadow_ppl.set_descriptor_sets(vec![tracer_shadow_ds.clone()]);
        vsm_creation_ppl.set_descriptor_sets(vec![vsm_ds_0.clone()]);
        vsm_blur_h_ppl.set_descriptor_sets(vec![vsm_ds_0.clone()]);
        vsm_blur_v_ppl.set_descriptor_sets(vec![vsm_ds_0.clone()]);

        let (grass_ppl, grass_render_pass) = Self::create_grass_render_pass_and_graphics_pipeline(
            &vulkan_ctx,
            &grass_vert_sm,
            &grass_frag_sm,
            resources.extent_dependent_resources.gfx_output_tex.clone(),
            resources.extent_dependent_resources.gfx_depth_tex.clone(),
        );

        let (shadow_ppl, shadow_render_pass) =
            Self::create_shadow_render_pass_and_graphics_pipeline(
                &vulkan_ctx,
                &shadow_vert_sm,
                &shadow_frag_sm,
                resources.shadow_map_tex.clone(),
            );

        // TODO: refac later
        let grass_ds = fixed_pool
            .allocate_set(&grass_ppl.get_layout().get_descriptor_set_layouts()[&0])
            .unwrap();
        Self::update_grass_ds(&grass_ds, &resources);

        let shadow_ds = fixed_pool
            .allocate_set(&shadow_ppl.get_layout().get_descriptor_set_layouts()[&0])
            .unwrap();
        Self::update_shadow_ds_0(&shadow_ds, &resources);

        let grass_framebuffer = Self::create_grass_framebuffer(
            &vulkan_ctx,
            &grass_render_pass,
            &resources.extent_dependent_resources.gfx_output_tex,
            &resources.extent_dependent_resources.gfx_depth_tex,
        );

        let shadow_framebuffer = Self::create_shadow_framebuffer(
            &vulkan_ctx,
            &shadow_render_pass,
            &resources.shadow_map_tex,
        );

        let mut this = Self {
            vulkan_ctx,
            desc,
            chunk_bound,
            allocator,
            resources,
            camera,
            camera_view_mat_prev_frame: Mat4::IDENTITY,
            camera_proj_mat_prev_frame: Mat4::IDENTITY,
            tracer_ppl,
            tracer_shadow_ppl,
            god_ray_ppl,
            temporal_ppl,
            spatial_ppl,
            composition_ppl,
            taa_ppl,
            post_processing_ppl,
            player_collider_ppl,
            vsm_creation_ppl,
            vsm_blur_h_ppl,
            vsm_blur_v_ppl,
            tracer_shadow_sets: [tracer_shadow_ds],
            grass_sets: [grass_ds],
            shadow_sets: [shadow_ds],
            grass_ppl,
            grass_render_pass,
            grass_framebuffer,
            shadow_ppl,
            shadow_render_pass,
            shadow_framebuffer,
            fixed_pool,
            flexible_pool,
            flexible_sets: vec![
                tracer_ds_1,
                god_ray_ds_0,
                composition_ds,
                taa_ds,
                post_processing_ds,
                denoiser_ds,
                temporal_ds,
                spatial_flexible_set,
            ],
        };
        this.update_flexible_sets();
        this
    }

    fn update_flexible_sets(&mut self) {
        Self::update_tracer_ds_1(&self.flexible_sets[0], &self.resources);
        Self::update_god_ray_ds_0(&self.flexible_sets[1], &self.resources);
        Self::update_composition_ds(&self.flexible_sets[2], &self.resources);
        Self::update_taa_ds(&self.flexible_sets[3], &self.resources);
        Self::update_post_processing_ds(&self.flexible_sets[4], &self.resources);
        Self::update_denoiser_ds(&self.flexible_sets[5], &self.resources);
        Self::update_temporal_ds(&self.flexible_sets[6], &self.resources);
        Self::update_spatial_flexible_set(&self.flexible_sets[7], &self.resources);
    }

    fn update_grass_ds(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.camera_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.shadow_camera_info),
            WriteDescriptorSet::new_buffer_write(2, &resources.grass_info),
            WriteDescriptorSet::new_buffer_write(3, &resources.gui_input),
            WriteDescriptorSet::new_texture_write(
                4,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                &resources.shadow_map_tex_for_vsm_ping,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    fn update_shadow_ds_0(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.shadow_camera_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.grass_info),
        ]);
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
                output_tex,
                Some(depth_tex),
                vk::AttachmentLoadOp::CLEAR,
                vk::ImageLayout::GENERAL,
                Some(vk::ImageLayout::GENERAL),
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
            Some(3),
        );

        (gfx_ppl, render_pass)
    }

    fn create_shadow_render_pass_and_graphics_pipeline(
        vulkan_ctx: &VulkanContext,
        vert_sm: &ShaderModule,
        frag_sm: &ShaderModule,
        shadow_depth_tex: Texture,
    ) -> (GraphicsPipeline, RenderPass) {
        let render_pass_desc = RenderPassDesc {
            attachments: vec![AttachmentDesc {
                format: shadow_depth_tex.get_image().get_desc().format,
                samples: vk::SampleCountFlags::TYPE_1,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::STORE,
                stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
                stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
                initial_layout: vk::ImageLayout::UNDEFINED,
                final_layout: vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
            }],
            subpasses: vec![SubpassDesc {
                color_attachments: vec![],
                depth_stencil_attachment: Some(AttachmentReference {
                    attachment: 0,
                    layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                }),
                ..Default::default()
            }],
            // Add a dependency to ensure writes to the depth buffer are complete
            // before any subsequent pass tries to read from it.
            dependencies: vec![vk::SubpassDependency::default()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT) // Or an earlier stage
                .dst_stage_mask(vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE)],
        };

        // 2. Create the render pass using the flexible `from_desc` constructor.
        let render_pass = RenderPass::from_desc(vulkan_ctx.device().clone(), render_pass_desc);

        // 3. Create the graphics pipeline for this render pass.
        let gfx_ppl = GraphicsPipeline::new(
            vulkan_ctx.device(),
            &vert_sm,
            &frag_sm,
            &render_pass,
            &GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK, // Or FRONT depending on your shadow bias needs
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
            Some(3),
        );

        (gfx_ppl, render_pass)
    }

    fn create_grass_framebuffer(
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

    fn create_shadow_framebuffer(
        vulkan_ctx: &VulkanContext,
        render_pass: &RenderPass,
        shadow_depth_tex: &Texture,
    ) -> Framebuffer {
        let shadow_depth_image_view = shadow_depth_tex.get_image_view().as_raw();
        let shadow_depth_image_extent = shadow_depth_tex
            .get_image()
            .get_desc()
            .extent
            .as_extent_2d()
            .unwrap();

        Framebuffer::new(
            vulkan_ctx.clone(),
            &render_pass,
            &[shadow_depth_image_view],
            shadow_depth_image_extent,
        )
        .unwrap()
    }

    fn update_tracer_ds_0(
        ds: &DescriptorSet,
        resources: &TracerResources,
        node_data: &Buffer,
        leaf_data: &Buffer,
        scene_tex: &Texture,
    ) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.gui_input),
            WriteDescriptorSet::new_buffer_write(1, &resources.camera_info),
            WriteDescriptorSet::new_buffer_write(2, &resources.camera_info_prev_frame),
            WriteDescriptorSet::new_buffer_write(3, &resources.shadow_camera_info),
            WriteDescriptorSet::new_buffer_write(4, &resources.env_info),
            WriteDescriptorSet::new_buffer_write(5, &node_data),
            WriteDescriptorSet::new_buffer_write(6, &leaf_data),
            WriteDescriptorSet::new_texture_write(
                7,
                vk::DescriptorType::STORAGE_IMAGE,
                &scene_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                8,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                &resources.shadow_map_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    fn update_tracer_ds_1(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_texture_write(
                0,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.compute_output_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.compute_depth_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    fn update_denoiser_ds(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_texture_write(
                0,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.normal,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.normal_prev,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.position,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.position_prev,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                4,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.vox_id,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                5,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.vox_id_prev,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                6,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.accumed,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                7,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.accumed_prev,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                8,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.motion,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                9,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.temporal_hist_len,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                10,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.hit,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                11,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.spatial_ping,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                12,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.spatial_pong,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    fn update_temporal_ds(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.denoiser_resources.temporal_info),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.compute_output_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    fn update_spatial_fixed_set(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [WriteDescriptorSet::new_buffer_write(
            0,
            &resources.denoiser_resources.spatial_info,
        )]);
    }

    fn update_spatial_flexible_set(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [WriteDescriptorSet::new_texture_write(
            0,
            vk::DescriptorType::STORAGE_IMAGE,
            &resources.extent_dependent_resources.compute_depth_tex,
            vk::ImageLayout::GENERAL,
        )]);
    }

    fn update_noise_tex_ds(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_texture_write(
                0,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.scalar_bn,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.unit_vec2_bn,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.unit_vec3_bn,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.weighted_cosine_bn,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                4,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.fast_unit_vec3_bn,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                5,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.fast_weighted_cosine_bn,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    fn update_god_ray_ds_0(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.camera_info),
            WriteDescriptorSet::new_buffer_write(1, &resources.shadow_camera_info),
            WriteDescriptorSet::new_buffer_write(2, &resources.gui_input),
            WriteDescriptorSet::new_buffer_write(3, &resources.env_info),
            WriteDescriptorSet::new_texture_write(
                4,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.gfx_depth_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                5,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.compute_depth_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                6,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                &resources.shadow_map_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                7,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.god_ray_output_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    fn update_composition_ds(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_texture_write(
                0,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.gfx_output_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.gfx_depth_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.spatial_pong,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.compute_depth_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                4,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.god_ray_output_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                5,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.composited_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    fn update_taa_ds(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.taa_info),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.composited_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.denoiser_resources.tex.motion,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.taa_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                4,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.taa_tex_prev,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    fn update_post_processing_ds(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.post_processing_info),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.taa_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.extent_dependent_resources.screen_output_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    fn update_tracer_shadow_ds(
        ds: &DescriptorSet,
        resources: &TracerResources,
        node_data: &Buffer,
        leaf_data: &Buffer,
        scene_tex: &Texture,
    ) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.shadow_camera_info),
            WriteDescriptorSet::new_buffer_write(1, &node_data),
            WriteDescriptorSet::new_buffer_write(2, &leaf_data),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &scene_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                4,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.shadow_map_tex,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    fn update_vsm_ds_0(ds: &DescriptorSet, resources: &TracerResources) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_texture_write(
                0,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.shadow_map_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                1,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.shadow_map_tex_for_vsm_ping,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_texture_write(
                2,
                vk::DescriptorType::STORAGE_IMAGE,
                &resources.shadow_map_tex_for_vsm_pong,
                vk::ImageLayout::GENERAL,
            ),
        ]);
    }

    fn update_player_collider_ds(
        ds: &DescriptorSet,
        resources: &TracerResources,
        node_data: &Buffer,
        leaf_data: &Buffer,
        scene_tex: &Texture,
    ) {
        ds.perform_writes(&mut [
            WriteDescriptorSet::new_buffer_write(0, &resources.player_collider_info),
            WriteDescriptorSet::new_buffer_write(1, &node_data),
            WriteDescriptorSet::new_buffer_write(2, &leaf_data),
            WriteDescriptorSet::new_texture_write(
                3,
                vk::DescriptorType::STORAGE_IMAGE,
                &scene_tex,
                vk::ImageLayout::GENERAL,
            ),
            WriteDescriptorSet::new_buffer_write(4, &resources.player_collision_result),
        ]);
    }

    pub fn on_resize(&mut self, screen_extent: Extent2D) {
        let render_extent = Self::get_render_extent(screen_extent, self.desc.scaling_factor);

        self.camera.on_resize(render_extent);

        // this must be done first
        self.resources.on_resize(
            self.vulkan_ctx.device().clone(),
            self.allocator.clone(),
            render_extent,
            screen_extent,
        );

        self.grass_framebuffer = Self::create_grass_framebuffer(
            &self.vulkan_ctx,
            &self.grass_render_pass,
            &self.resources.extent_dependent_resources.gfx_output_tex,
            &self.resources.extent_dependent_resources.gfx_depth_tex,
        );

        self.update_flexible_sets();
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
    ) -> Result<()> {
        // camera info
        update_cam_info(
            &mut self.resources.camera_info,
            self.camera.get_view_mat(),
            self.camera.get_proj_mat(),
        )?;

        // shadow cam info
        let world_bound = self.chunk_bound.into();
        let (shadow_view_mat, shadow_proj_mat) =
            calculate_directional_light_matrices(world_bound, sun_dir);
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

        update_post_processing_info(&self.resources, self.desc.scaling_factor)?;

        update_player_collider_info(&self.resources, self.camera.position())?;

        update_grass_info(&self.resources, time_info.time_since_start())?;

        update_gui_input(
            &self.resources,
            debug_float,
            debug_bool,
            debug_uint,
            sun_dir,
            sun_size,
            sun_color,
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
            sun_dir: Vec3,
            sun_size: f32,
            sun_color: Vec3,
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.gui_input)
                .set_field("debug_float", PlainMemberTypeWithData::Float(debug_float))
                .set_field(
                    "debug_bool",
                    PlainMemberTypeWithData::UInt(debug_bool as u32),
                )
                .set_field("debug_uint", PlainMemberTypeWithData::UInt(debug_uint))
                .set_field("sun_dir", PlainMemberTypeWithData::Vec3(sun_dir.to_array()))
                .set_field("sun_size", PlainMemberTypeWithData::Float(sun_size))
                .set_field(
                    "sun_color",
                    PlainMemberTypeWithData::Vec3(sun_color.to_array()),
                )
                .build()?;
            resources.gui_input.fill_with_raw_u8(&data)?;
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

        fn update_grass_info(resources: &TracerResources, time: f32) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.grass_info)
                .set_field("time", PlainMemberTypeWithData::Float(time))
                .build()?;
            resources.grass_info.fill_with_raw_u8(&data)?;
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
        ) -> Result<()> {
            let data = StructMemberDataBuilder::from_buffer(&resources.player_collider_info)
                .set_field(
                    "player_pos",
                    PlainMemberTypeWithData::Vec3(player_pos.to_array()),
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

        // let player_collision_result =
        //     get_player_collision_result(&self.resources.player_collision_result)?;

        // log::debug!("player_collision_result: {}", player_collision_result);

        return Ok(());

        fn record_denoiser_resources_transition_barrier(
            denoiser_resources: &DenoiserResources,
            cmdbuf: &CommandBuffer,
        ) {
            let tr_fn = |tex: &Texture| {
                tex.get_image()
                    .record_transition_barrier(cmdbuf, 0, vk::ImageLayout::GENERAL);
            };
            tr_fn(&denoiser_resources.tex.normal);
            tr_fn(&denoiser_resources.tex.normal_prev);
            tr_fn(&denoiser_resources.tex.position);
            tr_fn(&denoiser_resources.tex.position_prev);
            tr_fn(&denoiser_resources.tex.vox_id);
            tr_fn(&denoiser_resources.tex.vox_id_prev);
            tr_fn(&denoiser_resources.tex.accumed);
            tr_fn(&denoiser_resources.tex.accumed_prev);
            tr_fn(&denoiser_resources.tex.motion);
            tr_fn(&denoiser_resources.tex.temporal_hist_len);
            tr_fn(&denoiser_resources.tex.hit);
            tr_fn(&denoiser_resources.tex.spatial_ping);
            tr_fn(&denoiser_resources.tex.spatial_pong);
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
                &resources.denoiser_resources.tex.normal,
                &resources.denoiser_resources.tex.normal_prev,
            );
            copy_fn(
                &resources.denoiser_resources.tex.position,
                &resources.denoiser_resources.tex.position_prev,
            );
            copy_fn(
                &resources.denoiser_resources.tex.vox_id,
                &resources.denoiser_resources.tex.vox_id_prev,
            );
            copy_fn(
                &resources.denoiser_resources.tex.accumed,
                &resources.denoiser_resources.tex.accumed_prev,
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
            .record_begin(cmdbuf, &self.grass_framebuffer, &clear_values);

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
        self.grass_ppl
            .record_bind_descriptor_sets(cmdbuf, &self.grass_sets, 0);

        self.grass_ppl
            .record_viewport_scissor(cmdbuf, viewport, scissor);

        unsafe {
            // bind the index buffer
            self.vulkan_ctx.device().cmd_bind_index_buffer(
                cmdbuf.as_raw(),
                self.resources.indices.as_raw(),
                0,
                vk::IndexType::UINT32, // Use 32-bit indices
            );
        }

        // now, iterate over each chunk and issue a draw call for it.
        for (_chunk_id, chunk_resources) in &surface_resources.chunk_raster_resources {
            // only draw if this chunk actually has grass instances.
            if chunk_resources.grass_instances_len == 0 {
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
                        self.resources.vertices.as_raw(),
                        chunk_resources.grass_instances.as_raw(),
                    ],
                    &[0, 0], // offsets
                );
            }

            // issue the draw call for the current chunk.
            // No barriers are needed here.
            self.grass_ppl.record_draw_indexed(
                cmdbuf,
                self.resources.indices_len,
                chunk_resources.grass_instances_len,
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

    pub fn update_camera(&mut self, frame_delta_time: f32) {
        let res = get_player_collision_result(&self.resources.player_collision_result).unwrap();
        log::debug!("player_collision_result: {}", res);

        self.camera
            .update_transform_walk_mode(frame_delta_time, res);
        // self.camera.update_transform_fly_mode(frame_delta_time);

        fn get_player_collision_result(player_collision_result: &Buffer) -> Result<f32> {
            let layout = &player_collision_result.get_layout().unwrap().root_member;
            let raw_data = player_collision_result.read_back().unwrap();
            let reader = StructMemberDataReader::new(layout, &raw_data);

            let data =
                if let PlainMemberTypeWithData::Float(val) = reader.get_field("data").unwrap() {
                    val
                } else {
                    panic!("Expected Float type for player_collision_result");
                };
            Ok(data)
        }
    }
}
