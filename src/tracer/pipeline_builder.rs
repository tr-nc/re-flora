use crate::builder::{ContreeBuilderResources, PlainBuilderResources, SceneAccelBuilderResources};
use crate::ddgi::{DdgiConsumerResources, DdgiVolume, DdgiVoxelVisibility};
use crate::resource::ResourceContainer;
use crate::tracer::TracerResources;
use anyhow::{Context, Result};
use re_flora_vkn::vk;
use re_flora_vkn::{
    Allocator, AttachmentDescOuter, AttachmentType, ComputePipeline, DescriptorPool,
    DescriptorResource, DescriptorUpdate, DescriptorWrite, Extent2D, FrameExtentGeneration,
    FrameRetirement, FrameRetirementSink, Framebuffer, GraphicsPipeline, GraphicsPipelineDesc,
    PreparedDescriptorGeneration, RenderPass, RenderTarget, ShaderModule, Texture, TextureLayout,
    VulkanContext,
};

pub struct PipelineBuilder {
    shader_modules: ShaderModules,
}

impl PipelineBuilder {
    pub fn new(vulkan_ctx: &VulkanContext) -> Result<Self> {
        Ok(Self {
            shader_modules: Self::create_shader_modules(vulkan_ctx)?,
        })
    }

    pub fn shader_modules(&self) -> &ShaderModules {
        &self.shader_modules
    }

    pub fn build(self, input: PipelineTopologyBuild<'_>) -> PipelineTopology {
        let compute = Self::create_compute_pipelines(
            input.vulkan_ctx,
            &self.shader_modules,
            input.pool,
            input.resources,
            input.contree_builder_resources,
            input.scene_accel_resources,
            input.plain_builder_resources,
            input.ddgi_volume,
            input.ddgi_voxel_visibility,
        );
        let render_passes = Self::create_render_passes(
            input.vulkan_ctx,
            input
                .resources
                .extent_dependent_resources
                .gfx_output_tex
                .clone(),
            input
                .resources
                .extent_dependent_resources
                .gfx_depth_tex
                .clone(),
            input.resources.shadow.shadow_map_depth_tex.clone(),
            input.resources.shadow.leaf_shadow_opacity_tex.clone(),
        );
        let graphics = Self::create_graphics_pipelines(
            input.vulkan_ctx,
            &self.shader_modules,
            &render_passes,
            input.pool,
            input.resources,
            input.plain_builder_resources,
            input.ddgi_volume,
            input.ddgi_voxel_visibility,
        );
        let render_targets =
            PipelineRenderTargets::new(input.vulkan_ctx, render_passes, input.resources);

        PipelineTopology {
            compute,
            graphics,
            render_targets,
            frame_extent_generation: input.frame_extent_generation,
            frame_retirement_sink: input.frame_retirement_sink,
        }
    }

    pub fn create_shader_modules(vulkan_ctx: &VulkanContext) -> Result<ShaderModules> {
        let tracer_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/tracer.comp",
            "main",
        )
        .unwrap();
        let ddgi_global_sky_filter_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/ddgi/global_sky_filter.comp",
            "main",
        )
        .unwrap();
        let ddgi_octahedral_gutter_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/ddgi/octahedral_gutter.comp",
            "main",
        )
        .unwrap();
        let ddgi_probe_relocate_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/ddgi/probe_relocate.comp",
            "main",
        )
        .unwrap();
        let ddgi_probe_trace_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/ddgi/probe_trace.comp",
            "main",
        )
        .unwrap();
        let local_light_visibility_diagnostic_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/lighting/local_light_visibility_diagnostic.comp",
            "main",
        )
        .unwrap();
        let ddgi_irradiance_filter_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/ddgi/irradiance_filter.comp",
            "main",
        )
        .unwrap();
        let ddgi_visibility_filter_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/ddgi/visibility_filter.comp",
            "main",
        )
        .unwrap();
        let ddgi_irradiance_gutter_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/ddgi/irradiance_gutter.comp",
            "main",
        )
        .unwrap();
        let ddgi_visibility_gutter_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/ddgi/visibility_gutter.comp",
            "main",
        )
        .unwrap();
        let ddgi_atlas_reduce_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/ddgi/atlas_reduce.comp",
            "main",
        )
        .unwrap();
        let ddgi_voxel_visibility_pack_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/ddgi/voxel_visibility_pack.comp",
            "main",
        )
        .unwrap();
        let ddgi_voxel_visibility_blocks_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/ddgi/voxel_visibility_blocks.comp",
            "main",
        )
        .unwrap();

        let tracer_shadow_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/tracer_shadow.comp",
            "main",
        )
        .unwrap();

        let shadow_depth_copy_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/shadow_depth_copy.comp",
            "main",
        )
        .unwrap();

        let leaf_shadow_temporal_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/leaf_shadow_temporal.comp",
            "main",
        )
        .unwrap();

        let leaf_shadow_mask_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/leaf_shadow_mask.comp",
            "main",
        )
        .unwrap();

        let vsm_creation_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/vsm_creation.comp",
            "main",
        )
        .unwrap();

        let vsm_blur_h_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/vsm_blur_h.comp",
            "main",
        )
        .unwrap();

        let vsm_blur_v_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/vsm_blur_v.comp",
            "main",
        )
        .unwrap();

        let god_ray_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/god_ray.comp",
            "main",
        )
        .unwrap();

        let god_ray_temporal_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/god_ray_temporal.comp",
            "main",
        )
        .unwrap();

        let composition_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/composition.comp",
            "main",
        )
        .unwrap();
        let terrain_depth_prefill_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/terrain_depth_prefill.vert",
            "main",
        )
        .unwrap();
        let terrain_depth_prefill_frag_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/terrain_depth_prefill.frag",
            "main",
        )
        .unwrap();

        let cloud_sm =
            ShaderModule::from_precompiled(vulkan_ctx.device(), "shader/tracer/cloud.comp", "main")
                .unwrap();

        let cloud_shadow_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/cloud_shadow.comp",
            "main",
        )
        .unwrap();

        let cloud_shadow_temporal_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/cloud_shadow_temporal.comp",
            "main",
        )
        .unwrap();

        let cloud_temporal_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/cloud_temporal.comp",
            "main",
        )
        .unwrap();

        let lens_flare_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/lens_flare.comp",
            "main",
        )
        .unwrap();

        let lens_flare_temporal_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/lens_flare_temporal.comp",
            "main",
        )
        .unwrap();

        let lens_flare_sun_visible_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/lens_flare_sun_visible.comp",
            "main",
        )
        .unwrap();

        let post_processing_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/post_processing.comp",
            "main",
        )
        .unwrap();

        let player_collider_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/player_collider.comp",
            "main",
        )
        .unwrap();

        let terrain_query_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/terrain_query.comp",
            "main",
        )
        .unwrap();

        let wind_volume_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/tracer/wind_volume.comp",
            "main",
        )
        .unwrap();

        let flora_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/foliage/flora.vert",
            "main",
        )
        .unwrap();

        let flora_frag_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/foliage/flora.frag",
            "main",
        )
        .unwrap();

        let flora_lod_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/foliage/flora_lod.vert",
            "main",
        )
        .unwrap();
        let flora_lighting_cache_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/foliage/flora_lighting_cache.comp",
            "main",
        )
        .unwrap();
        let tree_leaf_lighting_cache_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/foliage/tree_leaf_lighting_cache.comp",
            "main",
        )
        .unwrap();

        let leaves_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/foliage/leaves.vert",
            "main",
        )
        .unwrap();

        let leaves_lod_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/foliage/leaves_lod.vert",
            "main",
        )
        .unwrap();

        let leaves_shadow_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/foliage/leaves_shadow.vert",
            "main",
        )
        .unwrap();

        let leaves_shadow_frag_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/foliage/leaves_shadow.frag",
            "main",
        )
        .unwrap();

        let sprinkler_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/props/sprinkler.vert",
            "main",
        )
        .unwrap();

        let geometry_preview_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/preview/geometry_preview.vert",
            "main",
        )
        .unwrap();
        let geometry_preview_frag_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/preview/geometry_preview.frag",
            "main",
        )
        .unwrap();
        let environment_probe_visualization_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/preview/environment_probe_visualization.vert",
            "main",
        )
        .unwrap();
        let dynamic_fruit_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/props/dynamic_fruit.vert",
            "main",
        )
        .unwrap();
        let dynamic_fruit_shadow_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/props/dynamic_fruit_shadow.vert",
            "main",
        )
        .unwrap();
        let dynamic_fruit_shadow_frag_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/props/dynamic_fruit_shadow.frag",
            "main",
        )
        .unwrap();

        let particle_lod_textured_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/particles/particle_lod_textured.vert",
            "main",
        )
        .unwrap();
        let particle_lod_textured_frag_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/particles/particle_lod_textured.frag",
            "main",
        )
        .unwrap();
        let water_droplet_frag_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/particles/water_droplet.frag",
            "main",
        )
        .unwrap();

        let glass_vert_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/terrarium/glass.vert",
            "main",
        )
        .unwrap();
        let glass_frag_sm = ShaderModule::from_precompiled(
            vulkan_ctx.device(),
            "shader/terrarium/glass.frag",
            "main",
        )
        .unwrap();

        Ok(ShaderModules {
            tracer_sm,
            ddgi_global_sky_filter_sm,
            ddgi_octahedral_gutter_sm,
            ddgi_probe_relocate_sm,
            ddgi_probe_trace_sm,
            local_light_visibility_diagnostic_sm,
            ddgi_irradiance_filter_sm,
            ddgi_visibility_filter_sm,
            ddgi_irradiance_gutter_sm,
            ddgi_visibility_gutter_sm,
            ddgi_atlas_reduce_sm,
            ddgi_voxel_visibility_pack_sm,
            ddgi_voxel_visibility_blocks_sm,
            tracer_shadow_sm,
            shadow_depth_copy_sm,
            leaf_shadow_temporal_sm,
            leaf_shadow_mask_sm,
            vsm_creation_sm,
            vsm_blur_h_sm,
            vsm_blur_v_sm,
            god_ray_sm,
            god_ray_temporal_sm,
            composition_sm,
            terrain_depth_prefill_vert_sm,
            terrain_depth_prefill_frag_sm,
            cloud_sm,
            cloud_shadow_sm,
            cloud_shadow_temporal_sm,
            cloud_temporal_sm,
            lens_flare_sm,
            lens_flare_temporal_sm,
            lens_flare_sun_visible_sm,
            post_processing_sm,
            player_collider_sm,
            terrain_query_sm,
            wind_volume_sm,
            flora_vert_sm,
            flora_frag_sm,
            flora_lod_vert_sm,
            flora_lighting_cache_sm,
            tree_leaf_lighting_cache_sm,
            leaves_vert_sm,
            leaves_lod_vert_sm,
            leaves_shadow_vert_sm,
            leaves_shadow_frag_sm,
            sprinkler_vert_sm,
            geometry_preview_vert_sm,
            geometry_preview_frag_sm,
            environment_probe_visualization_vert_sm,
            dynamic_fruit_vert_sm,
            dynamic_fruit_shadow_vert_sm,
            dynamic_fruit_shadow_frag_sm,
            particle_lod_textured_vert_sm,
            particle_lod_textured_frag_sm,
            water_droplet_frag_sm,
            glass_vert_sm,
            glass_frag_sm,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_compute_pipelines(
        vulkan_ctx: &VulkanContext,
        shader_modules: &ShaderModules,
        pool: &DescriptorPool,
        resources: &TracerResources,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
        plain_builder_resources: &PlainBuilderResources,
        ddgi_volume: &DdgiVolume,
        ddgi_voxel_visibility: &DdgiVoxelVisibility,
    ) -> ComputePipelines {
        let device = vulkan_ctx.device();

        let ddgi_global_sky_filter_ppl = ComputePipeline::new(
            device,
            &shader_modules.ddgi_global_sky_filter_sm,
            pool,
            &[resources, ddgi_volume],
        );
        let ddgi_octahedral_gutter_ppl = ComputePipeline::new(
            device,
            &shader_modules.ddgi_octahedral_gutter_sm,
            pool,
            &[ddgi_volume],
        );
        let ddgi_probe_relocate_ppl = ComputePipeline::new(
            device,
            &shader_modules.ddgi_probe_relocate_sm,
            pool,
            &[plain_builder_resources, ddgi_volume],
        );
        let ddgi_probe_trace_ppl = ComputePipeline::new(
            device,
            &shader_modules.ddgi_probe_trace_sm,
            pool,
            &[
                resources,
                contree_builder_resources,
                scene_accel_resources,
                ddgi_volume,
                ddgi_voxel_visibility,
            ],
        );
        let local_light_visibility_diagnostic_ppl = ComputePipeline::new(
            device,
            &shader_modules.local_light_visibility_diagnostic_sm,
            pool,
            &[resources, contree_builder_resources, scene_accel_resources],
        );
        let ddgi_irradiance_filter_ppl = ComputePipeline::new(
            device,
            &shader_modules.ddgi_irradiance_filter_sm,
            pool,
            &[ddgi_volume],
        );
        let ddgi_visibility_filter_ppl = ComputePipeline::new(
            device,
            &shader_modules.ddgi_visibility_filter_sm,
            pool,
            &[ddgi_volume],
        );
        let ddgi_irradiance_gutter_ppl = ComputePipeline::new(
            device,
            &shader_modules.ddgi_irradiance_gutter_sm,
            pool,
            &[ddgi_volume],
        );
        let ddgi_visibility_gutter_ppl = ComputePipeline::new(
            device,
            &shader_modules.ddgi_visibility_gutter_sm,
            pool,
            &[ddgi_volume],
        );
        let ddgi_atlas_reduce_ppl = ComputePipeline::new(
            device,
            &shader_modules.ddgi_atlas_reduce_sm,
            pool,
            &[ddgi_volume],
        );
        let ddgi_voxel_visibility_pack_ppl = ComputePipeline::new(
            device,
            &shader_modules.ddgi_voxel_visibility_pack_sm,
            pool,
            &[plain_builder_resources, ddgi_voxel_visibility],
        );
        let ddgi_voxel_visibility_blocks_ppl = ComputePipeline::new(
            device,
            &shader_modules.ddgi_voxel_visibility_blocks_sm,
            pool,
            &[ddgi_voxel_visibility],
        );
        let flora_lighting_cache_ppl = ComputePipeline::new_uninitialized(
            device,
            &shader_modules.flora_lighting_cache_sm,
            pool,
        );
        flora_lighting_cache_ppl
            .initialize_descriptors(DescriptorUpdate::SetContaining {
                anchor: "gui_input",
                providers: &[
                    resources,
                    contree_builder_resources,
                    scene_accel_resources,
                    plain_builder_resources,
                    ddgi_volume,
                    ddgi_voxel_visibility,
                ],
            })
            .expect("flora lighting cache static descriptors must resolve");
        let tree_leaf_lighting_cache_ppl = ComputePipeline::new_uninitialized(
            device,
            &shader_modules.tree_leaf_lighting_cache_sm,
            pool,
        );
        tree_leaf_lighting_cache_ppl
            .initialize_descriptors(DescriptorUpdate::SetContaining {
                anchor: "gui_input",
                providers: &[
                    resources,
                    contree_builder_resources,
                    scene_accel_resources,
                    plain_builder_resources,
                    ddgi_volume,
                    ddgi_voxel_visibility,
                ],
            })
            .expect("tree-leaf lighting cache static descriptors must resolve");
        let tracer_ppl = ComputePipeline::new(
            device,
            &shader_modules.tracer_sm,
            pool,
            &[
                resources,
                contree_builder_resources,
                scene_accel_resources,
                plain_builder_resources,
                ddgi_volume,
                ddgi_voxel_visibility,
            ],
        );

        let tracer_shadow_ppl = ComputePipeline::new(
            device,
            &shader_modules.tracer_shadow_sm,
            pool,
            &[resources, contree_builder_resources, scene_accel_resources],
        );

        let shadow_depth_copy_ppl = ComputePipeline::new(
            device,
            &shader_modules.shadow_depth_copy_sm,
            pool,
            &[resources],
        );

        let leaf_shadow_temporal_ppl = ComputePipeline::new(
            device,
            &shader_modules.leaf_shadow_temporal_sm,
            pool,
            &[resources],
        );

        let leaf_shadow_mask_ppl = ComputePipeline::new(
            device,
            &shader_modules.leaf_shadow_mask_sm,
            pool,
            &[resources],
        );

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

        let vsm_creation_ppl =
            ComputePipeline::new(device, &shader_modules.vsm_creation_sm, pool, &[resources]);
        let vsm_blur_h_ppl =
            ComputePipeline::new(device, &shader_modules.vsm_blur_h_sm, pool, &[resources]);
        let vsm_blur_v_ppl =
            ComputePipeline::new(device, &shader_modules.vsm_blur_v_sm, pool, &[resources]);
        let god_ray_ppl =
            ComputePipeline::new(device, &shader_modules.god_ray_sm, pool, &[resources]);
        let god_ray_temporal_ppl = ComputePipeline::new(
            device,
            &shader_modules.god_ray_temporal_sm,
            pool,
            &[resources],
        );
        let composition_ppl =
            ComputePipeline::new(device, &shader_modules.composition_sm, pool, &[resources]);
        let cloud_ppl = ComputePipeline::new(device, &shader_modules.cloud_sm, pool, &[resources]);
        let cloud_shadow_ppl =
            ComputePipeline::new(device, &shader_modules.cloud_shadow_sm, pool, &[resources]);
        let cloud_shadow_temporal_ppl = ComputePipeline::new(
            device,
            &shader_modules.cloud_shadow_temporal_sm,
            pool,
            &[resources],
        );
        let cloud_temporal_ppl = ComputePipeline::new(
            device,
            &shader_modules.cloud_temporal_sm,
            pool,
            &[resources],
        );
        let lens_flare_ppl =
            ComputePipeline::new(device, &shader_modules.lens_flare_sm, pool, &[resources]);
        let lens_flare_temporal_ppl = ComputePipeline::new(
            device,
            &shader_modules.lens_flare_temporal_sm,
            pool,
            &[resources],
        );
        let lens_flare_sun_visible_ppl = ComputePipeline::new(
            device,
            &shader_modules.lens_flare_sun_visible_sm,
            pool,
            &[resources],
        );
        let post_processing_ppl = ComputePipeline::new(
            device,
            &shader_modules.post_processing_sm,
            pool,
            &[resources],
        );

        ComputePipelines {
            ddgi_global_sky_filter_ppl,
            ddgi_octahedral_gutter_ppl,
            ddgi_probe_relocate_ppl,
            ddgi_probe_trace_ppl,
            local_light_visibility_diagnostic_ppl,
            ddgi_irradiance_filter_ppl,
            ddgi_visibility_filter_ppl,
            ddgi_irradiance_gutter_ppl,
            ddgi_visibility_gutter_ppl,
            ddgi_atlas_reduce_ppl,
            ddgi_voxel_visibility_pack_ppl,
            ddgi_voxel_visibility_blocks_ppl,
            flora_lighting_cache_ppl,
            tree_leaf_lighting_cache_ppl,
            tracer_ppl,
            tracer_shadow_ppl,
            shadow_depth_copy_ppl,
            leaf_shadow_temporal_ppl,
            leaf_shadow_mask_ppl,
            vsm_creation_ppl,
            vsm_blur_h_ppl,
            vsm_blur_v_ppl,
            god_ray_ppl,
            god_ray_temporal_ppl,
            cloud_ppl,
            cloud_shadow_ppl,
            cloud_shadow_temporal_ppl,
            cloud_temporal_ppl,
            lens_flare_ppl,
            lens_flare_temporal_ppl,
            lens_flare_sun_visible_ppl,
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
        leaf_shadow_opacity_tex: Texture,
    ) -> RenderPasses {
        let render_pass_color_and_depth = Self::create_render_pass_with_color_and_depth(
            vulkan_ctx,
            gfx_output_tex.clone(),
            gfx_depth_tex.clone(),
        );
        let render_pass_depth =
            Self::create_render_pass_with_depth(vulkan_ctx, shadow_map_depth_tex);
        let render_pass_leaf_shadow_opacity =
            Self::create_render_pass_with_color(vulkan_ctx, leaf_shadow_opacity_tex);
        RenderPasses {
            render_pass_color_and_depth,
            render_pass_depth,
            render_pass_leaf_shadow_opacity,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_graphics_pipelines(
        vulkan_ctx: &VulkanContext,
        shader_modules: &ShaderModules,
        render_passes: &RenderPasses,
        pool: &DescriptorPool,
        resources: &TracerResources,
        plain_builder_resources: &PlainBuilderResources,
        ddgi_volume: &DdgiVolume,
        ddgi_voxel_visibility: &DdgiVoxelVisibility,
    ) -> GraphicsPipelines {
        let flora_resources: [&dyn ResourceContainer; 4] = [
            resources,
            plain_builder_resources,
            ddgi_volume,
            ddgi_voxel_visibility,
        ];
        let environment_lighting_resources: [&dyn ResourceContainer; 3] =
            [resources, ddgi_volume, ddgi_voxel_visibility];
        let terrain_depth_prefill_ppl = Self::create_gfx_pipeline_with_desc(
            vulkan_ctx,
            &shader_modules.terrain_depth_prefill_vert_sm,
            &shader_modules.terrain_depth_prefill_frag_sm,
            &render_passes.render_pass_color_and_depth,
            None,
            pool,
            &[resources],
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::NONE,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
        );
        let flora_ppl = Self::create_gfx_pipeline_uninitialized(
            vulkan_ctx,
            &shader_modules.flora_vert_sm,
            &shader_modules.flora_frag_sm,
            &render_passes.render_pass_color_and_depth,
            None,
            pool,
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
        );
        flora_ppl
            .initialize_descriptors(DescriptorUpdate::SetContaining {
                anchor: "gui_input",
                providers: &flora_resources,
            })
            .expect("flora static descriptors must resolve from tracer resources");

        let flora_lod_ppl = Self::create_gfx_pipeline_uninitialized(
            vulkan_ctx,
            &shader_modules.flora_lod_vert_sm,
            &shader_modules.flora_frag_sm,
            &render_passes.render_pass_color_and_depth,
            None,
            pool,
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
        );
        flora_lod_ppl
            .initialize_descriptors(DescriptorUpdate::SetContaining {
                anchor: "gui_input",
                providers: &flora_resources,
            })
            .expect("flora LOD static descriptors must resolve from tracer resources");

        let leaves_ppl = Self::create_gfx_pipeline_uninitialized(
            vulkan_ctx,
            &shader_modules.leaves_vert_sm,
            &shader_modules.flora_frag_sm,
            &render_passes.render_pass_color_and_depth,
            None,
            pool,
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
        );
        leaves_ppl
            .initialize_descriptors(DescriptorUpdate::SetContaining {
                anchor: "gui_input",
                providers: &environment_lighting_resources,
            })
            .expect("leaf static descriptors must resolve from tracer resources");

        let leaves_lod_ppl = Self::create_gfx_pipeline_uninitialized(
            vulkan_ctx,
            &shader_modules.leaves_lod_vert_sm,
            &shader_modules.flora_frag_sm,
            &render_passes.render_pass_color_and_depth,
            None,
            pool,
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
        );
        leaves_lod_ppl
            .initialize_descriptors(DescriptorUpdate::SetContaining {
                anchor: "gui_input",
                providers: &environment_lighting_resources,
            })
            .expect("leaf LOD static descriptors must resolve from tracer resources");

        let leaves_shadow_lod_ppl = Self::create_gfx_pipeline_with_desc_uninitialized(
            vulkan_ctx,
            &shader_modules.leaves_shadow_vert_sm,
            &shader_modules.leaves_shadow_frag_sm,
            &render_passes.render_pass_leaf_shadow_opacity,
            None,
            pool,
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: false,
                depth_write_enable: false,
                ..Default::default()
            },
        );
        leaves_shadow_lod_ppl
            .initialize_descriptors(DescriptorUpdate::SetContaining {
                anchor: "gui_input",
                providers: &[resources],
            })
            .expect("leaf shadow static descriptors must resolve from tracer resources");

        let sprinkler_ppl = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.sprinkler_vert_sm,
            &shader_modules.flora_frag_sm,
            &render_passes.render_pass_color_and_depth,
            Some(5),
            pool,
            &environment_lighting_resources,
        );

        let geometry_preview_ppl = Self::create_gfx_pipeline_with_desc(
            vulkan_ctx,
            &shader_modules.geometry_preview_vert_sm,
            &shader_modules.geometry_preview_frag_sm,
            &render_passes.render_pass_color_and_depth,
            Some(2),
            pool,
            &[resources],
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                // The hybrid compositor only sees raster geometry with a real raster depth.
                depth_write_enable: true,
                ..Default::default()
            },
        );
        let environment_probe_visualization_resources: [&dyn ResourceContainer; 3] =
            [resources, ddgi_volume, ddgi_voxel_visibility];
        let environment_probe_visualization_depth_ppl = Self::create_gfx_pipeline_with_desc(
            vulkan_ctx,
            &shader_modules.environment_probe_visualization_vert_sm,
            &shader_modules.geometry_preview_frag_sm,
            &render_passes.render_pass_color_and_depth,
            None,
            pool,
            &environment_probe_visualization_resources,
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::NONE,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
        );
        let environment_probe_visualization_overlay_ppl = Self::create_gfx_pipeline_with_desc(
            vulkan_ctx,
            &shader_modules.environment_probe_visualization_vert_sm,
            &shader_modules.geometry_preview_frag_sm,
            &render_passes.render_pass_color_and_depth,
            None,
            pool,
            &environment_probe_visualization_resources,
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::NONE,
                depth_test_enable: false,
                depth_write_enable: true,
                ..Default::default()
            },
        );

        let dynamic_fruit_ppl = Self::create_gfx_pipeline_with_desc(
            vulkan_ctx,
            &shader_modules.dynamic_fruit_vert_sm,
            &shader_modules.flora_frag_sm,
            &render_passes.render_pass_color_and_depth,
            Some(4),
            pool,
            &environment_lighting_resources,
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
        );

        let dynamic_fruit_shadow_ppl = Self::create_gfx_pipeline_with_desc(
            vulkan_ctx,
            &shader_modules.dynamic_fruit_shadow_vert_sm,
            &shader_modules.dynamic_fruit_shadow_frag_sm,
            &render_passes.render_pass_depth,
            Some(4),
            pool,
            &[resources],
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
        );

        let particle_ppl = Self::create_gfx_pipeline(
            vulkan_ctx,
            &shader_modules.particle_lod_textured_vert_sm,
            &shader_modules.particle_lod_textured_frag_sm,
            &render_passes.render_pass_color_and_depth,
            Some(2),
            pool,
            &environment_lighting_resources,
        );
        let water_droplet_ppl = Self::create_gfx_pipeline_with_desc(
            vulkan_ctx,
            &shader_modules.particle_lod_textured_vert_sm,
            &shader_modules.water_droplet_frag_sm,
            &render_passes.render_pass_color_and_depth,
            Some(2),
            pool,
            &environment_lighting_resources,
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                // The hybrid compositor uses raster depth to place premultiplied raster color
                // over ray-traced terrain. Back-to-front sorting keeps droplet overlap valid even
                // though the nearest translucent droplet ultimately owns this depth pixel.
                depth_write_enable: true,
                ..Default::default()
            },
        );

        let glass_ppl = Self::create_gfx_pipeline_with_desc(
            vulkan_ctx,
            &shader_modules.glass_vert_sm,
            &shader_modules.glass_frag_sm,
            &render_passes.render_pass_color_and_depth,
            None,
            pool,
            &[resources],
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::NONE,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
        );

        GraphicsPipelines {
            terrain_depth_prefill_ppl,
            flora_ppl,
            flora_lod_ppl,
            leaves_ppl,
            leaves_lod_ppl,
            leaves_shadow_lod_ppl,
            sprinkler_ppl,
            geometry_preview_ppl,
            environment_probe_visualization_depth_ppl,
            environment_probe_visualization_overlay_ppl,
            dynamic_fruit_ppl,
            dynamic_fruit_shadow_ppl,
            particle_ppl,
            water_droplet_ppl,
            glass_ppl,
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

    fn create_render_pass_with_color(vulkan_ctx: &VulkanContext, color_tex: Texture) -> RenderPass {
        RenderPass::with_attachments(
            vulkan_ctx.device().clone(),
            &[AttachmentDescOuter {
                texture: color_tex,
                load_op: vk::AttachmentLoadOp::LOAD,
                store_op: vk::AttachmentStoreOp::STORE,
                initial_layout: TextureLayout::GENERAL,
                final_layout: TextureLayout::GENERAL,
                ty: AttachmentType::Color,
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
        Self::create_gfx_pipeline_with_desc(
            vulkan_ctx,
            vert_sm,
            frag_sm,
            render_pass,
            instance_rate_starting_location,
            descriptor_pool,
            resource_containers,
            GraphicsPipelineDesc {
                cull_mode: vk::CullModeFlags::BACK,
                depth_test_enable: true,
                depth_write_enable: true,
                ..Default::default()
            },
        )
    }

    fn create_gfx_pipeline_uninitialized(
        vulkan_ctx: &VulkanContext,
        vert_sm: &ShaderModule,
        frag_sm: &ShaderModule,
        render_pass: &RenderPass,
        instance_rate_starting_location: Option<u32>,
        descriptor_pool: &DescriptorPool,
        desc: GraphicsPipelineDesc,
    ) -> GraphicsPipeline {
        GraphicsPipeline::new_uninitialized(
            vulkan_ctx.device(),
            vert_sm,
            frag_sm,
            render_pass,
            &desc,
            instance_rate_starting_location,
            descriptor_pool,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create_gfx_pipeline_with_desc(
        vulkan_ctx: &VulkanContext,
        vert_sm: &ShaderModule,
        frag_sm: &ShaderModule,
        render_pass: &RenderPass,
        instance_rate_starting_location: Option<u32>,
        descriptor_pool: &DescriptorPool,
        resource_containers: &[&dyn ResourceContainer],
        desc: GraphicsPipelineDesc,
    ) -> GraphicsPipeline {
        GraphicsPipeline::new(
            vulkan_ctx.device(),
            vert_sm,
            frag_sm,
            render_pass,
            &desc,
            instance_rate_starting_location,
            descriptor_pool,
            resource_containers,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create_gfx_pipeline_with_desc_uninitialized(
        vulkan_ctx: &VulkanContext,
        vert_sm: &ShaderModule,
        frag_sm: &ShaderModule,
        render_pass: &RenderPass,
        instance_rate_starting_location: Option<u32>,
        descriptor_pool: &DescriptorPool,
        desc: GraphicsPipelineDesc,
    ) -> GraphicsPipeline {
        GraphicsPipeline::new_uninitialized(
            vulkan_ctx.device(),
            vert_sm,
            frag_sm,
            render_pass,
            &desc,
            instance_rate_starting_location,
            descriptor_pool,
        )
    }
}

pub struct PipelineTopologyBuild<'a> {
    pub vulkan_ctx: &'a VulkanContext,
    pub pool: &'a DescriptorPool,
    pub resources: &'a TracerResources,
    pub contree_builder_resources: &'a ContreeBuilderResources,
    pub scene_accel_resources: &'a SceneAccelBuilderResources,
    pub plain_builder_resources: &'a PlainBuilderResources,
    pub ddgi_volume: &'a DdgiVolume,
    pub ddgi_voxel_visibility: &'a DdgiVoxelVisibility,
    pub frame_extent_generation: FrameExtentGeneration,
    pub frame_retirement_sink: FrameRetirementSink,
}

pub struct PipelineTopology {
    compute: ComputePipelines,
    graphics: GraphicsPipelines,
    render_targets: PipelineRenderTargets,
    frame_extent_generation: FrameExtentGeneration,
    frame_retirement_sink: FrameRetirementSink,
}

macro_rules! declare_ddgi_consumer_registry {
    ($( $key:ident => $kind:ident($($path:ident).+) ),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(usize)]
        enum PipelineKey {
            $( $key, )+
            Count,
        }

        impl PipelineKey {
            const COUNT: usize = Self::Count as usize;
            const ALL: [Self; Self::COUNT] = [$( Self::$key, )+];
        }

        impl PipelineTopology {
            fn ddgi_consumer(&self, key: PipelineKey) -> DdgiConsumerPipeline<'_> {
                match key {
                    $(
                        PipelineKey::$key =>
                            DdgiConsumerPipeline::$kind(key, &self.$($path).+),
                    )+
                    PipelineKey::Count =>
                        unreachable!("registry sentinel is not a DDGI consumer"),
                }
            }
        }
    };
}

declare_ddgi_consumer_registry! {
    Tracer => Compute(compute.tracer_ppl),
    FloraLightingCache => Compute(compute.flora_lighting_cache_ppl),
    TreeLeafLightingCache => Compute(compute.tree_leaf_lighting_cache_ppl),
    Flora => Graphics(graphics.flora_ppl),
    FloraLod => Graphics(graphics.flora_lod_ppl),
    Leaves => Graphics(graphics.leaves_ppl),
    LeavesLod => Graphics(graphics.leaves_lod_ppl),
    Sprinkler => Graphics(graphics.sprinkler_ppl),
    DynamicFruit => Graphics(graphics.dynamic_fruit_ppl),
    Particle => Graphics(graphics.particle_ppl),
    WaterDroplet => Graphics(graphics.water_droplet_ppl),
    EnvironmentProbeDepth => Graphics(graphics.environment_probe_visualization_depth_ppl),
    EnvironmentProbeOverlay => Graphics(graphics.environment_probe_visualization_overlay_ppl),
}

fn validate_ddgi_consumer_registry(keys: &[PipelineKey]) -> Result<()> {
    anyhow::ensure!(
        keys.len() == PipelineKey::COUNT,
        "DDGI consumer registry is incomplete"
    );
    let mut seen = [false; PipelineKey::COUNT];
    for key in keys {
        let index = *key as usize;
        anyhow::ensure!(index < PipelineKey::COUNT, "invalid DDGI consumer key");
        anyhow::ensure!(!seen[index], "duplicate DDGI consumer key: {key:?}");
        seen[index] = true;
    }
    anyhow::ensure!(seen.into_iter().all(|present| present));
    Ok(())
}

fn validate_prepared_ddgi_consumer_keys(
    expected: &[PipelineKey],
    prepared: &[PipelineKey],
) -> Result<()> {
    anyhow::ensure!(
        prepared.len() == expected.len(),
        "prepared DDGI consumer topology is incomplete"
    );
    for (expected, prepared) in expected.iter().zip(prepared) {
        anyhow::ensure!(
            prepared == expected,
            "prepared DDGI consumer generation belongs to a different topology consumer"
        );
    }
    Ok(())
}

fn allocate_generation_after_preflight(
    next_generation: &mut u64,
    preflight: impl FnOnce() -> Result<()>,
) -> Result<u64> {
    preflight()?;
    let generation = *next_generation;
    *next_generation = generation
        .checked_add(1)
        .context("tracer descriptor generation overflow")?;
    Ok(generation)
}

struct PreparedDdgiConsumerGeneration {
    key: PipelineKey,
    descriptors: PreparedDescriptorGeneration,
}

#[derive(Clone, Copy)]
enum DdgiConsumerPipeline<'a> {
    Compute(PipelineKey, &'a ComputePipeline),
    Graphics(PipelineKey, &'a GraphicsPipeline),
}

impl DdgiConsumerPipeline<'_> {
    fn key(self) -> PipelineKey {
        match self {
            Self::Compute(key, _) | Self::Graphics(key, _) => key,
        }
    }

    fn prepare(self, writes: &[DescriptorWrite<'_>]) -> Result<PreparedDdgiConsumerGeneration> {
        let descriptors = match self {
            Self::Compute(_, pipeline) => {
                pipeline.prepare_descriptors(DescriptorUpdate::Named(writes))?
            }
            Self::Graphics(_, pipeline) => {
                pipeline.prepare_descriptors(DescriptorUpdate::Named(writes))?
            }
        };
        Ok(PreparedDdgiConsumerGeneration {
            key: self.key(),
            descriptors,
        })
    }

    fn validate(self, prepared: &PreparedDdgiConsumerGeneration) -> Result<()> {
        anyhow::ensure!(
            prepared.key == self.key(),
            "prepared DDGI consumer generation belongs to a different topology consumer"
        );
        match self {
            Self::Compute(_, pipeline) => {
                pipeline.validate_prepared_descriptors(&prepared.descriptors)
            }
            Self::Graphics(_, pipeline) => {
                pipeline.validate_prepared_descriptors(&prepared.descriptors)
            }
        }
    }

    fn publish(self, generation: u64, prepared: PreparedDdgiConsumerGeneration) -> FrameRetirement {
        match self {
            Self::Compute(_, pipeline) => pipeline.publish_prepared_descriptors(
                "ddgi.consumer.descriptors",
                generation,
                prepared.descriptors,
            ),
            Self::Graphics(_, pipeline) => pipeline.publish_prepared_descriptors(
                "ddgi.consumer.descriptors",
                generation,
                prepared.descriptors,
            ),
        }
    }
}

impl PipelineTopology {
    pub fn compute(&self) -> &ComputePipelines {
        &self.compute
    }

    pub fn graphics(&self) -> &GraphicsPipelines {
        &self.graphics
    }

    pub fn frame_extent_generation(&self) -> FrameExtentGeneration {
        self.frame_extent_generation
    }

    pub fn color_and_depth_target(&self) -> &RenderTarget {
        &self.render_targets.color_and_depth
    }

    pub fn depth_only_target(&self) -> &RenderTarget {
        &self.render_targets.depth_only
    }

    pub fn leaf_shadow_opacity_target(&self) -> &RenderTarget {
        &self.render_targets.leaf_shadow_opacity
    }

    #[allow(clippy::too_many_arguments)]
    pub fn publish_extent_generation(
        &mut self,
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        resources: &mut TracerResources,
        render_extent: Extent2D,
        frame_extent_generation: FrameExtentGeneration,
        environment_irradiance_capture_enabled: bool,
        descriptor_generation: u64,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
        plain_builder_resources: &PlainBuilderResources,
        active_ddgi_volume: &DdgiVolume,
        ddgi_voxel_visibility: &DdgiVoxelVisibility,
    ) {
        let expected_serial = self
            .frame_extent_generation
            .serial()
            .checked_add(1)
            .expect("pipeline topology frame extent generation overflow");
        assert_eq!(
            frame_extent_generation.serial(),
            expected_serial,
            "pipeline topology frame extent generation must advance exactly once"
        );

        let retired_resources = resources.replace_extent_dependent_resources(
            vulkan_ctx.device().clone(),
            allocator,
            render_extent,
            frame_extent_generation.extent(),
            environment_irradiance_capture_enabled,
        );
        let replacement_targets = self.render_targets.replacement(vulkan_ctx, resources);
        let retired_targets = std::mem::replace(&mut self.render_targets, replacement_targets);
        let retired_generation = self.frame_extent_generation.serial();
        self.frame_extent_generation = frame_extent_generation;

        self.publish_extent_descriptors(
            descriptor_generation,
            resources,
            contree_builder_resources,
            scene_accel_resources,
            plain_builder_resources,
            active_ddgi_volume,
            ddgi_voxel_visibility,
        );
        self.frame_retirement_sink.retire(FrameRetirement::new(
            "tracer.extent_dependent",
            retired_generation,
            (retired_resources, retired_targets),
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_extent_descriptors(
        &self,
        generation: u64,
        resources: &TracerResources,
        contree_builder_resources: &ContreeBuilderResources,
        scene_accel_resources: &SceneAccelBuilderResources,
        plain_builder_resources: &PlainBuilderResources,
        active_ddgi_volume: &DdgiVolume,
        ddgi_voxel_visibility: &DdgiVoxelVisibility,
    ) {
        let retire_compute = |pipeline: &ComputePipeline,
                              update: DescriptorUpdate<'_>,
                              error: &'static str| {
            self.frame_retirement_sink.retire(
                pipeline
                    .publish_descriptors("tracer.resize.compute.descriptors", generation, update)
                    .expect(error),
            );
        };
        let retire_graphics = |pipeline: &GraphicsPipeline,
                               update: DescriptorUpdate<'_>,
                               error: &'static str| {
            self.frame_retirement_sink.retire(
                pipeline
                    .publish_descriptors("tracer.resize.graphics.descriptors", generation, update)
                    .expect(error),
            );
        };

        let all_resources: [&dyn ResourceContainer; 6] = [
            resources,
            contree_builder_resources,
            scene_accel_resources,
            plain_builder_resources,
            active_ddgi_volume,
            ddgi_voxel_visibility,
        ];
        for pipeline in [
            &self.compute.tracer_ppl,
            &self.compute.tracer_shadow_ppl,
            &self.compute.player_collider_ppl,
            &self.compute.terrain_query_ppl,
        ] {
            retire_compute(
                pipeline,
                DescriptorUpdate::All(&all_resources),
                "compute descriptor update failed during extent publication",
            );
        }

        let tracer_resources: [&dyn ResourceContainer; 3] =
            [resources, active_ddgi_volume, ddgi_voxel_visibility];
        for pipeline in [
            &self.compute.wind_volume_ppl,
            &self.compute.shadow_depth_copy_ppl,
            &self.compute.leaf_shadow_mask_ppl,
            &self.compute.vsm_creation_ppl,
            &self.compute.vsm_blur_h_ppl,
            &self.compute.vsm_blur_v_ppl,
            &self.compute.god_ray_ppl,
            &self.compute.god_ray_temporal_ppl,
            &self.compute.cloud_ppl,
            &self.compute.cloud_shadow_ppl,
            &self.compute.cloud_shadow_temporal_ppl,
            &self.compute.cloud_temporal_ppl,
            &self.compute.lens_flare_ppl,
            &self.compute.lens_flare_temporal_ppl,
            &self.compute.lens_flare_sun_visible_ppl,
            &self.compute.composition_ppl,
            &self.compute.post_processing_ppl,
        ] {
            retire_compute(
                pipeline,
                DescriptorUpdate::All(&tracer_resources),
                "compute descriptor update failed during extent publication",
            );
        }

        let environment_lighting_resources: [&dyn ResourceContainer; 3] =
            [resources, active_ddgi_volume, ddgi_voxel_visibility];
        retire_graphics(
            &self.graphics.terrain_depth_prefill_ppl,
            DescriptorUpdate::All(&tracer_resources),
            "graphics descriptor update failed during extent publication",
        );
        for pipeline in [&self.graphics.flora_ppl, &self.graphics.flora_lod_ppl] {
            retire_graphics(
                pipeline,
                DescriptorUpdate::SetContaining {
                    anchor: "gui_input",
                    providers: &all_resources,
                },
                "graphics descriptor set update failed during extent publication",
            );
        }
        for pipeline in [&self.graphics.leaves_ppl, &self.graphics.leaves_lod_ppl] {
            retire_graphics(
                pipeline,
                DescriptorUpdate::SetContaining {
                    anchor: "gui_input",
                    providers: &environment_lighting_resources,
                },
                "graphics descriptor set update failed during extent publication",
            );
        }
        retire_graphics(
            &self.graphics.leaves_shadow_lod_ppl,
            DescriptorUpdate::SetContaining {
                anchor: "gui_input",
                providers: &tracer_resources,
            },
            "graphics descriptor set update failed during extent publication",
        );
        for pipeline in [
            &self.graphics.sprinkler_ppl,
            &self.graphics.environment_probe_visualization_depth_ppl,
            &self.graphics.environment_probe_visualization_overlay_ppl,
            &self.graphics.dynamic_fruit_ppl,
            &self.graphics.particle_ppl,
            &self.graphics.water_droplet_ppl,
        ] {
            retire_graphics(
                pipeline,
                DescriptorUpdate::All(&environment_lighting_resources),
                "graphics descriptor update failed during extent publication",
            );
        }
        retire_graphics(
            &self.graphics.geometry_preview_ppl,
            DescriptorUpdate::All(&tracer_resources),
            "graphics descriptor update failed during extent publication",
        );

        let ddgi_resources: [&dyn ResourceContainer; 2] = [resources, active_ddgi_volume];
        retire_compute(
            &self.compute.ddgi_global_sky_filter_ppl,
            DescriptorUpdate::All(&ddgi_resources),
            "DDGI global-sky descriptor update failed during extent publication",
        );
        retire_compute(
            &self.compute.ddgi_octahedral_gutter_ppl,
            DescriptorUpdate::All(&[active_ddgi_volume]),
            "DDGI octahedral-gutter descriptor update failed during extent publication",
        );
        retire_compute(
            &self.compute.ddgi_probe_relocate_ppl,
            DescriptorUpdate::All(&[plain_builder_resources, active_ddgi_volume]),
            "DDGI relocation descriptor update failed during extent publication",
        );
        retire_compute(
            &self.compute.ddgi_probe_trace_ppl,
            DescriptorUpdate::All(&[
                resources,
                contree_builder_resources,
                scene_accel_resources,
                active_ddgi_volume,
                ddgi_voxel_visibility,
            ]),
            "DDGI trace descriptor update failed during extent publication",
        );
        retire_compute(
            &self.compute.ddgi_voxel_visibility_pack_ppl,
            DescriptorUpdate::All(&[plain_builder_resources, ddgi_voxel_visibility]),
            "DDGI voxel pack descriptor update failed during extent publication",
        );
        retire_compute(
            &self.compute.ddgi_voxel_visibility_blocks_ppl,
            DescriptorUpdate::All(&[ddgi_voxel_visibility]),
            "DDGI voxel blocks descriptor update failed during extent publication",
        );
        for pipeline in [
            &self.compute.ddgi_irradiance_filter_ppl,
            &self.compute.ddgi_visibility_filter_ppl,
            &self.compute.ddgi_irradiance_gutter_ppl,
            &self.compute.ddgi_visibility_gutter_ppl,
            &self.compute.ddgi_atlas_reduce_ppl,
        ] {
            retire_compute(
                pipeline,
                DescriptorUpdate::All(&[active_ddgi_volume]),
                "DDGI filter descriptor update failed during extent publication",
            );
        }
    }

    pub fn publish_ddgi_builder_generation(
        &self,
        volume: &DdgiVolume,
        inherited_source: Option<&DdgiVolume>,
        generation: u64,
    ) {
        let mut relocate = Vec::new();
        let mut trace = Vec::new();
        let mut irradiance_filter = Vec::new();
        let mut visibility_filter = Vec::new();
        let mut atlas_reduce = Vec::new();
        let mut global_sky_filter = Vec::new();
        let mut octahedral_gutter = Vec::new();
        let mut irradiance_gutter = Vec::new();
        let mut visibility_gutter = Vec::new();

        macro_rules! write_buffer {
            ($writes:expr, $name:literal, $buffer:expr) => {
                $writes.push(DescriptorWrite {
                    name: $name,
                    resource: DescriptorResource::Buffer($buffer),
                });
            };
        }
        macro_rules! write_texture {
            ($writes:expr, $name:literal, $texture:expr) => {
                $writes.push(DescriptorWrite {
                    name: $name,
                    resource: DescriptorResource::Texture($texture),
                });
            };
        }

        write_buffer!(relocate, "ddgi_probe_metadata", &volume.ddgi_probe_metadata);
        write_buffer!(
            relocate,
            "ddgi_relocation_stats",
            &volume.ddgi_relocation_stats
        );
        for writes in [
            &mut trace,
            &mut irradiance_filter,
            &mut visibility_filter,
            &mut atlas_reduce,
        ] {
            write_buffer!(writes, "ddgi_probe_metadata", &volume.ddgi_probe_metadata);
        }
        for writes in [&mut trace, &mut irradiance_filter, &mut visibility_filter] {
            write_buffer!(
                writes,
                "ddgi_transient_ray_data",
                &volume.ddgi_transient_ray_data
            );
        }
        write_buffer!(trace, "ddgi_trace_stats", &volume.ddgi_trace_stats);
        write_buffer!(
            atlas_reduce,
            "ddgi_atlas_reduction",
            &volume.ddgi_atlas_reduction
        );
        write_buffer!(
            global_sky_filter,
            "ddgi_radiance_sun",
            &volume.ddgi_radiance_sun
        );
        write_buffer!(trace, "ddgi_radiance_sun", &volume.ddgi_radiance_sun);
        write_buffer!(
            trace,
            "ddgi_radiance_voxel_palette",
            &volume.ddgi_radiance_voxel_palette
        );
        write_buffer!(
            trace,
            "ddgi_transport_query_info",
            &volume.ddgi_transport_query_info
        );
        write_buffer!(
            trace,
            "ddgi_local_light_info",
            &volume.ddgi_local_light_info
        );
        write_buffer!(trace, "ddgi_local_lights", &volume.ddgi_local_lights);

        let source_irradiance = inherited_source
            .and_then(DdgiVolume::published_irradiance_atlas)
            .unwrap_or(&volume.ddgi_transport_source_irradiance_atlas);
        let source_visibility = inherited_source
            .and_then(DdgiVolume::published_visibility_atlas)
            .unwrap_or(&volume.ddgi_transport_source_visibility_atlas);
        write_texture!(
            trace,
            "ddgi_transport_source_irradiance_atlas",
            source_irradiance
        );
        write_texture!(
            trace,
            "ddgi_global_sky_irradiance",
            volume.building_global_sky_irradiance()
        );
        write_texture!(
            trace,
            "ddgi_irradiance_atlas",
            &volume.ddgi_irradiance_atlas
        );
        write_texture!(
            trace,
            "ddgi_visibility_atlas",
            &volume.ddgi_visibility_atlas
        );
        write_texture!(
            trace,
            "ddgi_transport_source_visibility_atlas",
            source_visibility
        );
        write_texture!(
            global_sky_filter,
            "ddgi_global_sky_irradiance",
            volume.building_global_sky_irradiance()
        );
        write_texture!(
            octahedral_gutter,
            "ddgi_global_sky_irradiance",
            volume.building_global_sky_irradiance()
        );
        for writes in [
            &mut irradiance_filter,
            &mut irradiance_gutter,
            &mut atlas_reduce,
        ] {
            write_texture!(
                writes,
                "ddgi_irradiance_atlas",
                &volume.ddgi_irradiance_atlas
            );
            write_texture!(
                writes,
                "ddgi_transport_source_irradiance_atlas",
                source_irradiance
            );
        }
        for writes in [&mut visibility_filter, &mut visibility_gutter] {
            write_texture!(
                writes,
                "ddgi_visibility_atlas",
                &volume.ddgi_visibility_atlas
            );
            write_texture!(
                writes,
                "ddgi_transport_source_visibility_atlas",
                source_visibility
            );
        }

        let publish =
            |pipeline: &ComputePipeline, writes: &[DescriptorWrite<'_>], error: &'static str| {
                self.frame_retirement_sink.retire(
                    pipeline
                        .publish_descriptors(
                            "ddgi.builder.descriptors",
                            generation,
                            DescriptorUpdate::Named(writes),
                        )
                        .expect(error),
                );
            };
        publish(
            &self.compute.ddgi_probe_relocate_ppl,
            &relocate,
            "DDGI relocation descriptor update failed",
        );
        publish(
            &self.compute.ddgi_probe_trace_ppl,
            &trace,
            "DDGI trace descriptor update failed",
        );
        publish(
            &self.compute.ddgi_irradiance_filter_ppl,
            &irradiance_filter,
            "DDGI irradiance filter descriptor update failed",
        );
        publish(
            &self.compute.ddgi_visibility_filter_ppl,
            &visibility_filter,
            "DDGI visibility filter descriptor update failed",
        );
        publish(
            &self.compute.ddgi_atlas_reduce_ppl,
            &atlas_reduce,
            "DDGI atlas reduction descriptor update failed",
        );
        publish(
            &self.compute.ddgi_global_sky_filter_ppl,
            &global_sky_filter,
            "DDGI global sky filter descriptor update failed",
        );
        publish(
            &self.compute.ddgi_octahedral_gutter_ppl,
            &octahedral_gutter,
            "DDGI octahedral gutter descriptor update failed",
        );
        publish(
            &self.compute.ddgi_irradiance_gutter_ppl,
            &irradiance_gutter,
            "DDGI irradiance gutter descriptor update failed",
        );
        publish(
            &self.compute.ddgi_visibility_gutter_ppl,
            &visibility_gutter,
            "DDGI visibility gutter descriptor update failed",
        );
    }

    /// The single owner registry for every pipeline that samples the consumer-visible DDGI Volume.
    /// Preparation, preflight, and publication all resolve this same fixed sequence.
    fn ddgi_consumers(&self) -> Result<Vec<DdgiConsumerPipeline<'_>>> {
        validate_ddgi_consumer_registry(&PipelineKey::ALL)?;
        Ok(PipelineKey::ALL
            .into_iter()
            .map(|key| self.ddgi_consumer(key))
            .collect())
    }

    fn prepare_ddgi_consumers(
        &self,
        resources: DdgiConsumerResources<'_>,
    ) -> Result<Vec<PreparedDdgiConsumerGeneration>> {
        let mut writes = vec![DescriptorWrite {
            name: "ddgi_probe_metadata",
            resource: DescriptorResource::Buffer(resources.probe_metadata),
        }];
        for (name, texture) in [
            (
                "ddgi_global_sky_irradiance",
                resources.global_sky_irradiance,
            ),
            ("ddgi_irradiance_atlas", resources.irradiance_atlas),
            ("ddgi_visibility_atlas", resources.visibility_atlas),
        ] {
            writes.push(DescriptorWrite {
                name,
                resource: DescriptorResource::Texture(texture),
            });
        }
        self.ddgi_consumers()?
            .into_iter()
            .map(|pipeline| {
                pipeline
                    .prepare(&writes)
                    .with_context(|| format!("prepare DDGI consumer {:?}", pipeline.key()))
            })
            .collect::<Result<Vec<_>>>()
    }

    /// Fully preflights every topology-owned consumer before allocating a generation or making
    /// the first descriptor generation visible. Publication after this boundary is infallible.
    pub fn publish_ddgi_consumers(
        &self,
        resources: DdgiConsumerResources<'_>,
        next_generation: &mut u64,
    ) -> Result<u64> {
        anyhow::ensure!(
            resources.field.field().geometry_revision() == resources.build_token.terrain_revision()
                && resources.field.field().spacing_voxels()
                    == resources.build_token.spacing_voxels(),
            "DDGI consumer resources do not match their build token"
        );
        let prepared = self.prepare_ddgi_consumers(resources)?;
        let consumers = self.ddgi_consumers()?;
        let expected_keys = consumers
            .iter()
            .map(|pipeline| pipeline.key())
            .collect::<Vec<_>>();
        let prepared_keys = prepared
            .iter()
            .map(|generation| generation.key)
            .collect::<Vec<_>>();
        let generation = allocate_generation_after_preflight(next_generation, || {
            validate_prepared_ddgi_consumer_keys(&expected_keys, &prepared_keys)?;
            for (pipeline, generation) in consumers.iter().copied().zip(&prepared) {
                pipeline.validate(generation)?;
            }
            Ok(())
        })?;
        for (pipeline, prepared) in consumers.into_iter().zip(prepared) {
            self.frame_retirement_sink
                .retire(pipeline.publish(generation, prepared));
        }
        Ok(generation)
    }
}

struct PipelineRenderTargets {
    color_and_depth: RenderTarget,
    depth_only: RenderTarget,
    leaf_shadow_opacity: RenderTarget,
    gui: RenderTarget,
}

impl PipelineRenderTargets {
    fn new(
        vulkan_ctx: &VulkanContext,
        render_passes: RenderPasses,
        resources: &TracerResources,
    ) -> Self {
        let framebuffer_color_and_depth = framebuffer_color_and_depth(
            vulkan_ctx,
            &render_passes.render_pass_color_and_depth,
            &resources.extent_dependent_resources.gfx_output_tex,
            &resources.extent_dependent_resources.gfx_depth_tex,
        );
        let framebuffer_depth_only = framebuffer_single_attachment(
            vulkan_ctx,
            &render_passes.render_pass_depth,
            &resources.shadow.shadow_map_depth_tex,
        );
        let framebuffer_leaf_shadow_opacity = framebuffer_single_attachment(
            vulkan_ctx,
            &render_passes.render_pass_leaf_shadow_opacity,
            &resources.shadow.leaf_shadow_opacity_tex,
        );

        let color_and_depth = RenderTarget::new(
            render_passes.render_pass_color_and_depth,
            vec![framebuffer_color_and_depth],
        );
        let depth_only = RenderTarget::new(
            render_passes.render_pass_depth,
            vec![framebuffer_depth_only],
        );
        let leaf_shadow_opacity = RenderTarget::new(
            render_passes.render_pass_leaf_shadow_opacity,
            vec![framebuffer_leaf_shadow_opacity],
        );
        let gui_render_pass = RenderPass::with_attachments(
            vulkan_ctx.device().clone(),
            &[AttachmentDescOuter {
                texture: resources
                    .extent_dependent_resources
                    .screenshot_output_tex
                    .clone(),
                load_op: vk::AttachmentLoadOp::LOAD,
                store_op: vk::AttachmentStoreOp::STORE,
                initial_layout: TextureLayout::GENERAL,
                final_layout: TextureLayout::GENERAL,
                ty: AttachmentType::Color,
            }],
        );
        let framebuffer_gui = framebuffer_single_attachment(
            vulkan_ctx,
            &gui_render_pass,
            &resources.extent_dependent_resources.screenshot_output_tex,
        );
        let gui = RenderTarget::new(gui_render_pass, vec![framebuffer_gui]);

        Self {
            color_and_depth,
            depth_only,
            leaf_shadow_opacity,
            gui,
        }
    }

    fn replacement(&self, vulkan_ctx: &VulkanContext, resources: &TracerResources) -> Self {
        let color_and_depth_pass = self.color_and_depth.get_render_pass().clone();
        let depth_only_pass = self.depth_only.get_render_pass().clone();
        let leaf_shadow_opacity_pass = self.leaf_shadow_opacity.get_render_pass().clone();
        let gui_pass = self.gui.get_render_pass().clone();

        Self {
            color_and_depth: RenderTarget::new(
                color_and_depth_pass.clone(),
                vec![framebuffer_color_and_depth(
                    vulkan_ctx,
                    &color_and_depth_pass,
                    &resources.extent_dependent_resources.gfx_output_tex,
                    &resources.extent_dependent_resources.gfx_depth_tex,
                )],
            ),
            depth_only: RenderTarget::new(
                depth_only_pass.clone(),
                vec![framebuffer_single_attachment(
                    vulkan_ctx,
                    &depth_only_pass,
                    &resources.shadow.shadow_map_depth_tex,
                )],
            ),
            leaf_shadow_opacity: RenderTarget::new(
                leaf_shadow_opacity_pass.clone(),
                vec![framebuffer_single_attachment(
                    vulkan_ctx,
                    &leaf_shadow_opacity_pass,
                    &resources.shadow.leaf_shadow_opacity_tex,
                )],
            ),
            gui: RenderTarget::new(
                gui_pass.clone(),
                vec![framebuffer_single_attachment(
                    vulkan_ctx,
                    &gui_pass,
                    &resources.extent_dependent_resources.screenshot_output_tex,
                )],
            ),
        }
    }
}

fn framebuffer_color_and_depth(
    vulkan_ctx: &VulkanContext,
    render_pass: &RenderPass,
    target_texture: &Texture,
    depth_texture: &Texture,
) -> Framebuffer {
    let extent = target_texture
        .get_image()
        .get_desc()
        .extent
        .as_extent_2d()
        .unwrap();
    Framebuffer::from_textures(
        vulkan_ctx.clone(),
        render_pass,
        &[target_texture, depth_texture],
        extent,
    )
    .unwrap()
}

fn framebuffer_single_attachment(
    vulkan_ctx: &VulkanContext,
    render_pass: &RenderPass,
    texture: &Texture,
) -> Framebuffer {
    let extent = texture
        .get_image()
        .get_desc()
        .extent
        .as_extent_2d()
        .unwrap();
    Framebuffer::from_textures(vulkan_ctx.clone(), render_pass, &[texture], extent).unwrap()
}

pub struct ShaderModules {
    pub tracer_sm: ShaderModule,
    pub ddgi_global_sky_filter_sm: ShaderModule,
    pub ddgi_octahedral_gutter_sm: ShaderModule,
    pub ddgi_probe_relocate_sm: ShaderModule,
    pub ddgi_probe_trace_sm: ShaderModule,
    pub local_light_visibility_diagnostic_sm: ShaderModule,
    pub ddgi_irradiance_filter_sm: ShaderModule,
    pub ddgi_visibility_filter_sm: ShaderModule,
    pub ddgi_irradiance_gutter_sm: ShaderModule,
    pub ddgi_visibility_gutter_sm: ShaderModule,
    pub ddgi_atlas_reduce_sm: ShaderModule,
    pub ddgi_voxel_visibility_pack_sm: ShaderModule,
    pub ddgi_voxel_visibility_blocks_sm: ShaderModule,
    pub tracer_shadow_sm: ShaderModule,
    pub shadow_depth_copy_sm: ShaderModule,
    pub leaf_shadow_temporal_sm: ShaderModule,
    pub leaf_shadow_mask_sm: ShaderModule,
    pub vsm_creation_sm: ShaderModule,
    pub vsm_blur_h_sm: ShaderModule,
    pub vsm_blur_v_sm: ShaderModule,
    pub god_ray_sm: ShaderModule,
    pub god_ray_temporal_sm: ShaderModule,
    pub composition_sm: ShaderModule,
    pub terrain_depth_prefill_vert_sm: ShaderModule,
    pub terrain_depth_prefill_frag_sm: ShaderModule,
    pub cloud_sm: ShaderModule,
    pub cloud_shadow_sm: ShaderModule,
    pub cloud_shadow_temporal_sm: ShaderModule,
    pub cloud_temporal_sm: ShaderModule,
    pub lens_flare_sm: ShaderModule,
    pub lens_flare_temporal_sm: ShaderModule,
    pub lens_flare_sun_visible_sm: ShaderModule,
    pub post_processing_sm: ShaderModule,
    pub player_collider_sm: ShaderModule,
    pub terrain_query_sm: ShaderModule,
    pub wind_volume_sm: ShaderModule,
    pub flora_vert_sm: ShaderModule,
    pub flora_frag_sm: ShaderModule,
    pub flora_lod_vert_sm: ShaderModule,
    pub flora_lighting_cache_sm: ShaderModule,
    pub tree_leaf_lighting_cache_sm: ShaderModule,
    pub leaves_vert_sm: ShaderModule,
    pub leaves_lod_vert_sm: ShaderModule,
    pub leaves_shadow_vert_sm: ShaderModule,
    pub leaves_shadow_frag_sm: ShaderModule,
    pub sprinkler_vert_sm: ShaderModule,
    pub geometry_preview_vert_sm: ShaderModule,
    pub geometry_preview_frag_sm: ShaderModule,
    pub environment_probe_visualization_vert_sm: ShaderModule,
    pub dynamic_fruit_vert_sm: ShaderModule,
    pub dynamic_fruit_shadow_vert_sm: ShaderModule,
    pub dynamic_fruit_shadow_frag_sm: ShaderModule,
    pub particle_lod_textured_vert_sm: ShaderModule,
    pub particle_lod_textured_frag_sm: ShaderModule,
    pub water_droplet_frag_sm: ShaderModule,
    pub glass_vert_sm: ShaderModule,
    pub glass_frag_sm: ShaderModule,
}

pub struct ComputePipelines {
    pub ddgi_global_sky_filter_ppl: ComputePipeline,
    pub ddgi_octahedral_gutter_ppl: ComputePipeline,
    pub ddgi_probe_relocate_ppl: ComputePipeline,
    pub ddgi_probe_trace_ppl: ComputePipeline,
    pub local_light_visibility_diagnostic_ppl: ComputePipeline,
    pub ddgi_irradiance_filter_ppl: ComputePipeline,
    pub ddgi_visibility_filter_ppl: ComputePipeline,
    pub ddgi_irradiance_gutter_ppl: ComputePipeline,
    pub ddgi_visibility_gutter_ppl: ComputePipeline,
    pub ddgi_atlas_reduce_ppl: ComputePipeline,
    pub ddgi_voxel_visibility_pack_ppl: ComputePipeline,
    pub ddgi_voxel_visibility_blocks_ppl: ComputePipeline,
    pub flora_lighting_cache_ppl: ComputePipeline,
    pub tree_leaf_lighting_cache_ppl: ComputePipeline,
    pub tracer_ppl: ComputePipeline,
    pub tracer_shadow_ppl: ComputePipeline,
    pub shadow_depth_copy_ppl: ComputePipeline,
    pub leaf_shadow_temporal_ppl: ComputePipeline,
    pub leaf_shadow_mask_ppl: ComputePipeline,
    pub vsm_creation_ppl: ComputePipeline,
    pub vsm_blur_h_ppl: ComputePipeline,
    pub vsm_blur_v_ppl: ComputePipeline,
    pub god_ray_ppl: ComputePipeline,
    pub god_ray_temporal_ppl: ComputePipeline,
    pub cloud_ppl: ComputePipeline,
    pub cloud_shadow_ppl: ComputePipeline,
    pub cloud_shadow_temporal_ppl: ComputePipeline,
    pub cloud_temporal_ppl: ComputePipeline,
    pub lens_flare_ppl: ComputePipeline,
    pub lens_flare_temporal_ppl: ComputePipeline,
    pub lens_flare_sun_visible_ppl: ComputePipeline,
    pub composition_ppl: ComputePipeline,
    pub player_collider_ppl: ComputePipeline,
    pub terrain_query_ppl: ComputePipeline,
    pub wind_volume_ppl: ComputePipeline,
    pub post_processing_ppl: ComputePipeline,
}

pub struct RenderPasses {
    pub render_pass_color_and_depth: RenderPass,
    pub render_pass_depth: RenderPass,
    pub render_pass_leaf_shadow_opacity: RenderPass,
}

pub struct GraphicsPipelines {
    pub terrain_depth_prefill_ppl: GraphicsPipeline,
    pub flora_ppl: GraphicsPipeline,
    pub flora_lod_ppl: GraphicsPipeline,
    pub leaves_ppl: GraphicsPipeline,
    pub leaves_lod_ppl: GraphicsPipeline,
    pub leaves_shadow_lod_ppl: GraphicsPipeline,
    pub sprinkler_ppl: GraphicsPipeline,
    pub geometry_preview_ppl: GraphicsPipeline,
    pub environment_probe_visualization_depth_ppl: GraphicsPipeline,
    pub environment_probe_visualization_overlay_ppl: GraphicsPipeline,
    pub dynamic_fruit_ppl: GraphicsPipeline,
    pub dynamic_fruit_shadow_ppl: GraphicsPipeline,
    pub particle_ppl: GraphicsPipeline,
    pub water_droplet_ppl: GraphicsPipeline,
    pub glass_ppl: GraphicsPipeline,
}

impl GraphicsPipelines {
    pub fn begin_transient_descriptor_frame(&self, frame_slot: usize) {
        self.flora_ppl.begin_transient_descriptor_frame(frame_slot);
        self.flora_lod_ppl
            .begin_transient_descriptor_frame(frame_slot);
        self.leaves_ppl.begin_transient_descriptor_frame(frame_slot);
        self.leaves_lod_ppl
            .begin_transient_descriptor_frame(frame_slot);
        self.leaves_shadow_lod_ppl
            .begin_transient_descriptor_frame(frame_slot);
    }
}

#[cfg(test)]
mod topology_tests {
    use super::{
        allocate_generation_after_preflight, validate_ddgi_consumer_registry,
        validate_prepared_ddgi_consumer_keys, PipelineKey,
    };

    #[test]
    fn ddgi_consumer_registry_is_complete_and_unique() {
        validate_ddgi_consumer_registry(&PipelineKey::ALL).unwrap();

        let omitted = &PipelineKey::ALL[..PipelineKey::COUNT - 1];
        assert!(validate_ddgi_consumer_registry(omitted).is_err());

        let mut duplicate = PipelineKey::ALL;
        duplicate[PipelineKey::COUNT - 1] = PipelineKey::Tracer;
        assert!(validate_ddgi_consumer_registry(&duplicate).is_err());
    }

    #[test]
    fn prepared_ddgi_consumer_keys_reject_omission_and_identity_corruption() {
        validate_prepared_ddgi_consumer_keys(&PipelineKey::ALL, &PipelineKey::ALL).unwrap();

        let omitted = &PipelineKey::ALL[..PipelineKey::COUNT - 1];
        assert!(validate_prepared_ddgi_consumer_keys(&PipelineKey::ALL, omitted).is_err());

        let mut corrupted = PipelineKey::ALL;
        corrupted[4] = PipelineKey::Tracer;
        assert!(validate_prepared_ddgi_consumer_keys(&PipelineKey::ALL, &corrupted).is_err());
    }

    #[test]
    fn failed_consumer_preflight_consumes_no_generation_or_publication() {
        let mut next_generation = 41;
        let mut published = 0;
        let result = allocate_generation_after_preflight(&mut next_generation, || {
            anyhow::bail!("injected descriptor ownership failure")
        });
        if result.is_ok() {
            published += PipelineKey::COUNT;
        }

        assert!(result.is_err());
        assert_eq!(next_generation, 41);
        assert_eq!(published, 0);
    }
}
