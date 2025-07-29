use crate::builder::{ContreeBuilderResources, SceneAccelBuilderResources};
use crate::resource::ResourceContainer;
use crate::tracer::TracerResources;
use crate::util::ShaderCompiler;
use crate::vkn::{
    AttachmentDescOuter, AttachmentType, ComputePipeline, DescriptorPool, GraphicsPipeline,
    GraphicsPipelineDesc, RenderPass, ShaderModule, Texture, VulkanContext,
};
use anyhow::Result;
use ash::vk;

pub struct PipelineBuilder;

impl PipelineBuilder {
    pub fn create_shader_modules(
        vulkan_ctx: &VulkanContext,
        shader_compiler: &ShaderCompiler,
    ) -> Result<ShaderModules> {
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

        let flora_vert_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/flora.vert",
            "main",
        )
        .unwrap();

        let flora_frag_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/flora.frag",
            "main",
        )
        .unwrap();

        let flora_lod_vert_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/flora_lod.vert",
            "main",
        )
        .unwrap();

        let flora_lod_frag_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/flora_lod.frag",
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

        Ok(ShaderModules {
            tracer_sm,
            tracer_shadow_sm,
            vsm_creation_sm,
            vsm_blur_h_sm,
            vsm_blur_v_sm,
            god_ray_sm,
            temporal_sm,
            spatial_sm,
            composition_sm,
            taa_sm,
            post_processing_sm,
            player_collider_sm,
            terrain_query_sm,
            flora_vert_sm,
            flora_frag_sm,
            flora_lod_vert_sm,
            flora_lod_frag_sm,
            leaves_shadow_vert_sm,
            leaves_shadow_frag_sm,
        })
    }

    pub fn create_compute_pipelines(
        vulkan_ctx: &VulkanContext,
        shader_modules: &ShaderModules,
        pool: &DescriptorPool,
        resources: &TracerResources,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
    ) -> ComputePipelines {
        let device = vulkan_ctx.device();

        let tracer_ppl = ComputePipeline::new(
            device,
            &shader_modules.tracer_sm,
            pool,
            &[resources, contree_builder_resources, scene_accel_resources],
        );

        let tracer_shadow_ppl = ComputePipeline::new(
            device,
            &shader_modules.tracer_shadow_sm,
            pool,
            &[resources, contree_builder_resources, scene_accel_resources],
        );

        let vsm_creation_ppl =
            ComputePipeline::new(device, &shader_modules.vsm_creation_sm, pool, &[resources]);
        let vsm_blur_h_ppl =
            ComputePipeline::new(device, &shader_modules.vsm_blur_h_sm, pool, &[resources]);
        let vsm_blur_v_ppl =
            ComputePipeline::new(device, &shader_modules.vsm_blur_v_sm, pool, &[resources]);
        let god_ray_ppl =
            ComputePipeline::new(device, &shader_modules.god_ray_sm, pool, &[resources]);
        let temporal_ppl =
            ComputePipeline::new(device, &shader_modules.temporal_sm, pool, &[resources]);
        let spatial_ppl =
            ComputePipeline::new(device, &shader_modules.spatial_sm, pool, &[resources]);
        let composition_ppl =
            ComputePipeline::new(device, &shader_modules.composition_sm, pool, &[resources]);
        let taa_ppl = ComputePipeline::new(device, &shader_modules.taa_sm, pool, &[resources]);

        let player_collider_ppl = ComputePipeline::new(
            device,
            &shader_modules.player_collider_sm,
            pool,
            &[resources, contree_builder_resources, scene_accel_resources],
        );

        let terrain_query_ppl = ComputePipeline::new(
            device,
            &shader_modules.terrain_query_sm,
            pool,
            &[resources, contree_builder_resources, scene_accel_resources],
        );

        let post_processing_ppl = ComputePipeline::new(
            device,
            &shader_modules.post_processing_sm,
            pool,
            &[resources],
        );

        ComputePipelines {
            tracer_ppl,
            tracer_shadow_ppl,
            vsm_creation_ppl,
            vsm_blur_h_ppl,
            vsm_blur_v_ppl,
            god_ray_ppl,
            temporal_ppl,
            spatial_ppl,
            composition_ppl,
            taa_ppl,
            player_collider_ppl,
            terrain_query_ppl,
            post_processing_ppl,
        }
    }

    pub fn create_render_passes(
        vulkan_ctx: &VulkanContext,
        gfx_output_tex: Texture,
        gfx_depth_tex: Texture,
        shadow_map_tex: Texture,
    ) -> RenderPasses {
        let clear_render_pass_color_and_depth = Self::create_render_pass_with_color_and_depth(
            vulkan_ctx,
            gfx_output_tex.clone(),
            gfx_depth_tex.clone(),
            true,
        );

        let load_render_pass_color_and_depth = Self::create_render_pass_with_color_and_depth(
            vulkan_ctx,
            gfx_output_tex,
            gfx_depth_tex,
            false,
        );

        let clear_render_pass_depth =
            Self::create_render_pass_with_depth(vulkan_ctx, shadow_map_tex, true);

        RenderPasses {
            clear_render_pass_color_and_depth,
            load_render_pass_color_and_depth,
            clear_render_pass_depth,
        }
    }

    pub fn create_graphics_pipelines(
        vulkan_ctx: &VulkanContext,
        shader_modules: &ShaderModules,
        render_passes: &RenderPasses,
        pool: &DescriptorPool,
        resources: &TracerResources,
    ) -> GraphicsPipelines {
        let flora_ppl_with_clear = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.flora_vert_sm,
            &shader_modules.flora_frag_sm,
            &render_passes.clear_render_pass_color_and_depth,
            Some(1),
            pool,
            &[resources],
        );

        let flora_ppl_with_load = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.flora_vert_sm,
            &shader_modules.flora_frag_sm,
            &render_passes.load_render_pass_color_and_depth,
            Some(1),
            pool,
            &[resources],
        );

        let flora_lod_ppl_with_clear = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.flora_lod_vert_sm,
            &shader_modules.flora_lod_frag_sm,
            &render_passes.clear_render_pass_color_and_depth,
            Some(1),
            pool,
            &[resources],
        );

        let flora_lod_ppl_with_load = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.flora_lod_vert_sm,
            &shader_modules.flora_lod_frag_sm,
            &render_passes.load_render_pass_color_and_depth,
            Some(1),
            pool,
            &[resources],
        );

        let leaves_shadow_ppl_with_clear = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.leaves_shadow_vert_sm,
            &shader_modules.leaves_shadow_frag_sm,
            &render_passes.clear_render_pass_depth,
            Some(1),
            pool,
            &[resources],
        );

        GraphicsPipelines {
            flora_ppl_with_clear,
            flora_ppl_with_load,
            flora_lod_ppl_with_clear,
            flora_lod_ppl_with_load,
            leaves_shadow_ppl_with_clear,
        }
    }

    fn create_render_pass_with_color_and_depth(
        vulkan_ctx: &VulkanContext,
        output_tex: Texture,
        depth_tex: Texture,
        is_starting_with_clear: bool,
    ) -> RenderPass {
        if is_starting_with_clear {
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
        } else {
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
        }
    }

    fn create_render_pass_with_depth(
        vulkan_ctx: &VulkanContext,
        depth_tex: Texture,
        is_starting_with_clear: bool,
    ) -> RenderPass {
        if is_starting_with_clear {
            RenderPass::with_attachments(
                vulkan_ctx.device().clone(),
                &[AttachmentDescOuter {
                    texture: depth_tex,
                    load_op: vk::AttachmentLoadOp::CLEAR,
                    store_op: vk::AttachmentStoreOp::STORE,
                    initial_layout: vk::ImageLayout::UNDEFINED,
                    final_layout: vk::ImageLayout::GENERAL,
                    ty: AttachmentType::Depth,
                }],
            )
        } else {
            RenderPass::with_attachments(
                vulkan_ctx.device().clone(),
                &[AttachmentDescOuter {
                    texture: depth_tex,
                    load_op: vk::AttachmentLoadOp::LOAD,
                    store_op: vk::AttachmentStoreOp::STORE,
                    initial_layout: vk::ImageLayout::GENERAL,
                    final_layout: vk::ImageLayout::GENERAL,
                    ty: AttachmentType::Depth,
                }],
            )
        }
    }

    fn create_gfx_pipeline(
        vulkan_ctx: &VulkanContext,
        vert_sm: &ShaderModule,
        frag_sm: &ShaderModule,
        render_pass: &RenderPass,
        instance_rate_starting_location: Option<u32>,
        descriptor_pool: &DescriptorPool,
        resource_containers: &[&dyn ResourceContainer],
    ) -> GraphicsPipeline {
        GraphicsPipeline::new(
            vulkan_ctx.device(),
            vert_sm,
            frag_sm,
            render_pass,
            &GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
            instance_rate_starting_location,
            descriptor_pool,
            resource_containers,
        )
    }
}

pub struct ShaderModules {
    pub tracer_sm: ShaderModule,
    pub tracer_shadow_sm: ShaderModule,
    pub vsm_creation_sm: ShaderModule,
    pub vsm_blur_h_sm: ShaderModule,
    pub vsm_blur_v_sm: ShaderModule,
    pub god_ray_sm: ShaderModule,
    pub temporal_sm: ShaderModule,
    pub spatial_sm: ShaderModule,
    pub composition_sm: ShaderModule,
    pub taa_sm: ShaderModule,
    pub post_processing_sm: ShaderModule,
    pub player_collider_sm: ShaderModule,
    pub terrain_query_sm: ShaderModule,
    pub flora_vert_sm: ShaderModule,
    pub flora_frag_sm: ShaderModule,
    pub flora_lod_vert_sm: ShaderModule,
    pub flora_lod_frag_sm: ShaderModule,
    pub leaves_shadow_vert_sm: ShaderModule,
    pub leaves_shadow_frag_sm: ShaderModule,
}

pub struct ComputePipelines {
    pub tracer_ppl: ComputePipeline,
    pub tracer_shadow_ppl: ComputePipeline,
    pub vsm_creation_ppl: ComputePipeline,
    pub vsm_blur_h_ppl: ComputePipeline,
    pub vsm_blur_v_ppl: ComputePipeline,
    pub god_ray_ppl: ComputePipeline,
    pub temporal_ppl: ComputePipeline,
    pub spatial_ppl: ComputePipeline,
    pub composition_ppl: ComputePipeline,
    pub taa_ppl: ComputePipeline,
    pub player_collider_ppl: ComputePipeline,
    pub terrain_query_ppl: ComputePipeline,
    pub post_processing_ppl: ComputePipeline,
}

pub struct RenderPasses {
    pub clear_render_pass_color_and_depth: RenderPass,
    pub load_render_pass_color_and_depth: RenderPass,
    pub clear_render_pass_depth: RenderPass,
}

pub struct GraphicsPipelines {
    pub flora_ppl_with_clear: GraphicsPipeline,
    pub flora_ppl_with_load: GraphicsPipeline,
    pub flora_lod_ppl_with_clear: GraphicsPipeline,
    pub flora_lod_ppl_with_load: GraphicsPipeline,
    pub leaves_shadow_ppl_with_clear: GraphicsPipeline,
}
