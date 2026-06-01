use crate::builder::{ContreeBuilderResources, SceneAccelBuilderResources};
use crate::resource::ResourceContainer;
use crate::tracer::TracerResources;
use crate::util::ShaderCompiler;
use anyhow::Result;
use re_flora_vkn::vk;
use re_flora_vkn::{
    AttachmentDescOuter, AttachmentType, ComputePipeline, DescriptorPool, GraphicsPipeline,
    GraphicsPipelineDesc, RenderPass, ShaderModule, Texture, TextureLayout, VulkanContext,
};

pub struct PipelineBuilder;

impl PipelineBuilder {
    pub fn create_shader_modules(
        vulkan_ctx: &VulkanContext,
        shader_compiler: &ShaderCompiler,
    ) -> Result<ShaderModules> {
        let tracer_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/tracer.comp",
            "main",
        )
        .unwrap();

        let tracer_shadow_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/tracer_shadow.comp",
            "main",
        )
        .unwrap();

        let shadow_depth_copy_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/shadow_depth_copy.comp",
            "main",
        )
        .unwrap();

        let vsm_creation_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/vsm_creation.comp",
            "main",
        )
        .unwrap();

        let vsm_blur_h_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/vsm_blur_h.comp",
            "main",
        )
        .unwrap();

        let vsm_blur_v_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/vsm_blur_v.comp",
            "main",
        )
        .unwrap();

        let god_ray_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/god_ray.comp",
            "main",
        )
        .unwrap();

        let temporal_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/denoiser/temporal.comp",
            "main",
        )
        .unwrap();

        let spatial_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/denoiser/spatial.comp",
            "main",
        )
        .unwrap();

        let composition_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/composition.comp",
            "main",
        )
        .unwrap();

        let lens_flare_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/lens_flare.comp",
            "main",
        )
        .unwrap();

        let lens_flare_sun_visible_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/lens_flare_sun_visible.comp",
            "main",
        )
        .unwrap();

        let lens_flare_downsample_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/lens_flare_downsample.comp",
            "main",
        )
        .unwrap();

        let post_processing_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/post_processing.comp",
            "main",
        )
        .unwrap();

        let player_collider_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/player_collider.comp",
            "main",
        )
        .unwrap();

        let terrain_query_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/terrain_query.comp",
            "main",
        )
        .unwrap();

        let wind_volume_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/tracer/wind_volume.comp",
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

        let leaves_vert_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/leaves.vert",
            "main",
        )
        .unwrap();

        let leaves_lod_vert_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/foliage/leaves_lod.vert",
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

        let particle_lod_textured_vert_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/particles/particle_lod_textured.vert",
            "main",
        )
        .unwrap();
        let particle_lod_textured_frag_sm = ShaderModule::from_glsl(
            vulkan_ctx.device(),
            shader_compiler,
            "shader/particles/particle_lod_textured.frag",
            "main",
        )
        .unwrap();
        Ok(ShaderModules {
            tracer_sm,
            tracer_shadow_sm,
            shadow_depth_copy_sm,
            vsm_creation_sm,
            vsm_blur_h_sm,
            vsm_blur_v_sm,
            god_ray_sm,
            temporal_sm,
            spatial_sm,
            composition_sm,
            lens_flare_sm,
            lens_flare_sun_visible_sm,
            lens_flare_downsample_sm,
            post_processing_sm,
            player_collider_sm,
            terrain_query_sm,
            wind_volume_sm,
            flora_vert_sm,
            flora_frag_sm,
            flora_lod_vert_sm,
            leaves_vert_sm,
            leaves_lod_vert_sm,
            leaves_shadow_vert_sm,
            leaves_shadow_frag_sm,
            particle_lod_textured_vert_sm,
            particle_lod_textured_frag_sm,
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
        tracer_ppl.set_auto_texture_transitions_enabled(true);

        let tracer_shadow_ppl = ComputePipeline::new(
            device,
            &shader_modules.tracer_shadow_sm,
            pool,
            &[resources, contree_builder_resources, scene_accel_resources],
        );
        tracer_shadow_ppl.set_auto_texture_transitions_enabled(true);

        let shadow_depth_copy_ppl = ComputePipeline::new(
            device,
            &shader_modules.shadow_depth_copy_sm,
            pool,
            &[resources],
        );
        shadow_depth_copy_ppl.set_auto_texture_transitions_enabled(true);

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

        let wind_volume_ppl =
            ComputePipeline::new(device, &shader_modules.wind_volume_sm, pool, &[resources]);
        wind_volume_ppl.set_auto_texture_transitions_enabled(true);

        let vsm_creation_ppl =
            ComputePipeline::new(device, &shader_modules.vsm_creation_sm, pool, &[resources]);
        vsm_creation_ppl.set_auto_texture_transitions_enabled(true);
        let vsm_blur_h_ppl =
            ComputePipeline::new(device, &shader_modules.vsm_blur_h_sm, pool, &[resources]);
        vsm_blur_h_ppl.set_auto_texture_transitions_enabled(true);
        let vsm_blur_v_ppl =
            ComputePipeline::new(device, &shader_modules.vsm_blur_v_sm, pool, &[resources]);
        vsm_blur_v_ppl.set_auto_texture_transitions_enabled(true);
        let god_ray_ppl =
            ComputePipeline::new(device, &shader_modules.god_ray_sm, pool, &[resources]);
        god_ray_ppl.set_auto_texture_transitions_enabled(true);
        let temporal_ppl =
            ComputePipeline::new(device, &shader_modules.temporal_sm, pool, &[resources]);
        temporal_ppl.set_auto_texture_transitions_enabled(true);
        let spatial_ppl =
            ComputePipeline::new(device, &shader_modules.spatial_sm, pool, &[resources]);
        spatial_ppl.set_auto_texture_transitions_enabled(true);
        let composition_ppl =
            ComputePipeline::new(device, &shader_modules.composition_sm, pool, &[resources]);
        composition_ppl.set_auto_texture_transitions_enabled(true);
        let lens_flare_ppl =
            ComputePipeline::new(device, &shader_modules.lens_flare_sm, pool, &[resources]);
        lens_flare_ppl.set_auto_texture_transitions_enabled(true);
        let lens_flare_sun_visible_ppl = ComputePipeline::new(
            device,
            &shader_modules.lens_flare_sun_visible_sm,
            pool,
            &[resources],
        );
        lens_flare_sun_visible_ppl.set_auto_texture_transitions_enabled(true);
        let lens_flare_downsample_ppl = ComputePipeline::new(
            device,
            &shader_modules.lens_flare_downsample_sm,
            pool,
            &[resources],
        );
        lens_flare_downsample_ppl.set_auto_texture_transitions_enabled(true);

        let post_processing_ppl = ComputePipeline::new(
            device,
            &shader_modules.post_processing_sm,
            pool,
            &[resources],
        );
        post_processing_ppl.set_auto_texture_transitions_enabled(true);

        ComputePipelines {
            tracer_ppl,
            tracer_shadow_ppl,
            shadow_depth_copy_ppl,
            vsm_creation_ppl,
            vsm_blur_h_ppl,
            vsm_blur_v_ppl,
            god_ray_ppl,
            temporal_ppl,
            spatial_ppl,
            lens_flare_ppl,
            lens_flare_sun_visible_ppl,
            lens_flare_downsample_ppl,
            composition_ppl,
            player_collider_ppl,
            terrain_query_ppl,
            wind_volume_ppl,
            post_processing_ppl,
        }
    }

    pub fn create_render_passes(
        vulkan_ctx: &VulkanContext,
        gfx_output_tex: Texture,
        gfx_depth_tex: Texture,
        shadow_map_depth_tex: Texture,
    ) -> RenderPasses {
        let render_pass_color_and_depth = Self::create_render_pass_with_color_and_depth(
            vulkan_ctx,
            gfx_output_tex.clone(),
            gfx_depth_tex.clone(),
        );
        let render_pass_depth =
            Self::create_render_pass_with_depth(vulkan_ctx, shadow_map_depth_tex);
        RenderPasses {
            render_pass_color_and_depth,
            render_pass_depth,
        }
    }

    pub fn create_graphics_pipelines(
        vulkan_ctx: &VulkanContext,
        shader_modules: &ShaderModules,
        render_passes: &RenderPasses,
        pool: &DescriptorPool,
        resources: &TracerResources,
    ) -> GraphicsPipelines {
        let flora_ppl = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.flora_vert_sm,
            &shader_modules.flora_frag_sm,
            &render_passes.render_pass_color_and_depth,
            None,
            pool,
            &[resources],
        );

        let flora_lod_ppl = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.flora_lod_vert_sm,
            &shader_modules.flora_frag_sm,
            &render_passes.render_pass_color_and_depth,
            None,
            pool,
            &[resources],
        );

        let leaves_ppl = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.leaves_vert_sm,
            &shader_modules.flora_frag_sm,
            &render_passes.render_pass_color_and_depth,
            None,
            pool,
            &[resources],
        );

        let leaves_lod_ppl = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.leaves_lod_vert_sm,
            &shader_modules.flora_frag_sm,
            &render_passes.render_pass_color_and_depth,
            None,
            pool,
            &[resources],
        );

        let leaves_shadow_lod_ppl = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.leaves_shadow_vert_sm,
            &shader_modules.leaves_shadow_frag_sm,
            &render_passes.render_pass_depth,
            None,
            pool,
            &[resources],
        );

        let particle_ppl = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.particle_lod_textured_vert_sm,
            &shader_modules.particle_lod_textured_frag_sm,
            &render_passes.render_pass_color_and_depth,
            Some(1),
            pool,
            &[resources],
        );
        GraphicsPipelines {
            flora_ppl,
            flora_lod_ppl,
            leaves_ppl,
            leaves_lod_ppl,
            leaves_shadow_lod_ppl,
            particle_ppl,
        }
    }

    fn create_render_pass_with_color_and_depth(
        vulkan_ctx: &VulkanContext,
        output_tex: Texture,
        depth_tex: Texture,
    ) -> RenderPass {
        // CLEAR instead of LOAD: on tile-based GPUs (Apple Silicon via MoltenVK),
        // LOAD forces a full DRAM-to-tile read. Keeping this pass on CLEAR avoids
        // pulling previous attachment contents back into tile memory.
        RenderPass::with_attachments(
            vulkan_ctx.device().clone(),
            &[
                AttachmentDescOuter {
                    texture: output_tex,
                    load_op: vk::AttachmentLoadOp::CLEAR,
                    store_op: vk::AttachmentStoreOp::STORE,
                    initial_layout: TextureLayout::UNDEFINED,
                    final_layout: TextureLayout::GENERAL,
                    ty: AttachmentType::Color,
                },
                AttachmentDescOuter {
                    texture: depth_tex,
                    load_op: vk::AttachmentLoadOp::CLEAR,
                    store_op: vk::AttachmentStoreOp::STORE,
                    initial_layout: TextureLayout::UNDEFINED,
                    final_layout: TextureLayout::GENERAL,
                    ty: AttachmentType::Depth,
                },
            ],
        )
    }

    fn create_render_pass_with_depth(vulkan_ctx: &VulkanContext, depth_tex: Texture) -> RenderPass {
        RenderPass::with_attachments(
            vulkan_ctx.device().clone(),
            &[AttachmentDescOuter {
                texture: depth_tex,
                load_op: vk::AttachmentLoadOp::LOAD,
                store_op: vk::AttachmentStoreOp::STORE,
                initial_layout: TextureLayout::GENERAL,
                final_layout: TextureLayout::GENERAL,
                ty: AttachmentType::Depth,
            }],
        )
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
    pub shadow_depth_copy_sm: ShaderModule,
    pub vsm_creation_sm: ShaderModule,
    pub vsm_blur_h_sm: ShaderModule,
    pub vsm_blur_v_sm: ShaderModule,
    pub god_ray_sm: ShaderModule,
    pub temporal_sm: ShaderModule,
    pub spatial_sm: ShaderModule,
    pub composition_sm: ShaderModule,
    pub lens_flare_sm: ShaderModule,
    pub lens_flare_sun_visible_sm: ShaderModule,
    pub lens_flare_downsample_sm: ShaderModule,
    pub post_processing_sm: ShaderModule,
    pub player_collider_sm: ShaderModule,
    pub terrain_query_sm: ShaderModule,
    pub wind_volume_sm: ShaderModule,
    pub flora_vert_sm: ShaderModule,
    pub flora_frag_sm: ShaderModule,
    pub flora_lod_vert_sm: ShaderModule,
    pub leaves_vert_sm: ShaderModule,
    pub leaves_lod_vert_sm: ShaderModule,
    pub leaves_shadow_vert_sm: ShaderModule,
    pub leaves_shadow_frag_sm: ShaderModule,
    pub particle_lod_textured_vert_sm: ShaderModule,
    pub particle_lod_textured_frag_sm: ShaderModule,
}

pub struct ComputePipelines {
    pub tracer_ppl: ComputePipeline,
    pub tracer_shadow_ppl: ComputePipeline,
    pub shadow_depth_copy_ppl: ComputePipeline,
    pub vsm_creation_ppl: ComputePipeline,
    pub vsm_blur_h_ppl: ComputePipeline,
    pub vsm_blur_v_ppl: ComputePipeline,
    pub god_ray_ppl: ComputePipeline,
    pub temporal_ppl: ComputePipeline,
    pub spatial_ppl: ComputePipeline,
    pub lens_flare_ppl: ComputePipeline,
    pub lens_flare_sun_visible_ppl: ComputePipeline,
    pub lens_flare_downsample_ppl: ComputePipeline,
    pub composition_ppl: ComputePipeline,
    pub player_collider_ppl: ComputePipeline,
    pub terrain_query_ppl: ComputePipeline,
    pub wind_volume_ppl: ComputePipeline,
    pub post_processing_ppl: ComputePipeline,
}

pub struct RenderPasses {
    pub render_pass_color_and_depth: RenderPass,
    pub render_pass_depth: RenderPass,
}

pub struct GraphicsPipelines {
    pub flora_ppl: GraphicsPipeline,
    pub flora_lod_ppl: GraphicsPipeline,
    pub leaves_ppl: GraphicsPipeline,
    pub leaves_lod_ppl: GraphicsPipeline,
    pub leaves_shadow_lod_ppl: GraphicsPipeline,
    pub particle_ppl: GraphicsPipeline,
}
