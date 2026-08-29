use crate::builder::{ContreeBuilderResources, PlainBuilderResources, SceneAccelBuilderResources};
use crate::ddgi::{DdgiVolume, DdgiVoxelVisibility};
use crate::resource::ResourceContainer;
use crate::tracer::TracerResources;
use anyhow::Result;
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PipelineKind {
    Compute,
    Graphics,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[allow(dead_code)]
enum PipelineKey {
    DdgiGlobalSkyFilter,
    DdgiOctahedralGutter,
    DdgiProbeRelocate,
    DdgiProbeTrace,
    LocalLightVisibilityDiagnostic,
    DdgiIrradianceFilter,
    DdgiVisibilityFilter,
    DdgiIrradianceGutter,
    DdgiVisibilityGutter,
    DdgiAtlasReduce,
    DdgiVoxelVisibilityPack,
    DdgiVoxelVisibilityBlocks,
    FloraLightingCache,
    TreeLeafLightingCache,
    Tracer,
    TracerShadow,
    ShadowDepthCopy,
    LeafShadowTemporal,
    LeafShadowMask,
    VsmCreation,
    VsmBlurH,
    VsmBlurV,
    GodRay,
    GodRayTemporal,
    Cloud,
    CloudShadow,
    CloudShadowTemporal,
    CloudTemporal,
    LensFlare,
    LensFlareTemporal,
    LensFlareSunVisible,
    Composition,
    PlayerCollider,
    TerrainQuery,
    WindVolume,
    PostProcessing,
    TerrainDepthPrefill,
    Flora,
    FloraLod,
    Leaves,
    LeavesLod,
    LeavesShadowLod,
    Sprinkler,
    GeometryPreview,
    EnvironmentProbeDepth,
    EnvironmentProbeOverlay,
    DynamicFruit,
    DynamicFruitShadow,
    Particle,
    WaterDroplet,
    Glass,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LifecycleMask(u8);

#[cfg(test)]
impl LifecycleMask {
    const CONSTRUCTION: u8 = 1 << 0;
    const RECORD: u8 = 1 << 1;
    const EXTENT: u8 = 1 << 2;
    const DDGI_BUILDER: u8 = 1 << 3;
    const DDGI_CONSUMER: u8 = 1 << 4;

    const fn new(extra: u8) -> Self {
        Self(Self::CONSTRUCTION | Self::RECORD | extra)
    }

    const fn contains(self, lifecycle: u8) -> bool {
        self.0 & lifecycle != 0
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PipelineLifecycleSpec {
    key: PipelineKey,
    kind: PipelineKind,
    lifecycle: LifecycleMask,
}

#[cfg(test)]
macro_rules! spec {
    ($key:ident, $kind:ident, $extra:expr) => {
        PipelineLifecycleSpec {
            key: PipelineKey::$key,
            kind: PipelineKind::$kind,
            lifecycle: LifecycleMask::new($extra),
        }
    };
}

#[cfg(test)]
const PIPELINE_LIFECYCLE: &[PipelineLifecycleSpec] = &[
    spec!(
        DdgiGlobalSkyFilter,
        Compute,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_BUILDER
    ),
    spec!(
        DdgiOctahedralGutter,
        Compute,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_BUILDER
    ),
    spec!(
        DdgiProbeRelocate,
        Compute,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_BUILDER
    ),
    spec!(
        DdgiProbeTrace,
        Compute,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_BUILDER
    ),
    spec!(LocalLightVisibilityDiagnostic, Compute, 0),
    spec!(
        DdgiIrradianceFilter,
        Compute,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_BUILDER
    ),
    spec!(
        DdgiVisibilityFilter,
        Compute,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_BUILDER
    ),
    spec!(
        DdgiIrradianceGutter,
        Compute,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_BUILDER
    ),
    spec!(
        DdgiVisibilityGutter,
        Compute,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_BUILDER
    ),
    spec!(
        DdgiAtlasReduce,
        Compute,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_BUILDER
    ),
    spec!(DdgiVoxelVisibilityPack, Compute, LifecycleMask::EXTENT),
    spec!(DdgiVoxelVisibilityBlocks, Compute, LifecycleMask::EXTENT),
    spec!(FloraLightingCache, Compute, LifecycleMask::DDGI_CONSUMER),
    spec!(TreeLeafLightingCache, Compute, LifecycleMask::DDGI_CONSUMER),
    spec!(
        Tracer,
        Compute,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_CONSUMER
    ),
    spec!(TracerShadow, Compute, LifecycleMask::EXTENT),
    spec!(ShadowDepthCopy, Compute, LifecycleMask::EXTENT),
    spec!(LeafShadowTemporal, Compute, 0),
    spec!(LeafShadowMask, Compute, LifecycleMask::EXTENT),
    spec!(VsmCreation, Compute, LifecycleMask::EXTENT),
    spec!(VsmBlurH, Compute, LifecycleMask::EXTENT),
    spec!(VsmBlurV, Compute, LifecycleMask::EXTENT),
    spec!(GodRay, Compute, LifecycleMask::EXTENT),
    spec!(GodRayTemporal, Compute, LifecycleMask::EXTENT),
    spec!(Cloud, Compute, LifecycleMask::EXTENT),
    spec!(CloudShadow, Compute, LifecycleMask::EXTENT),
    spec!(CloudShadowTemporal, Compute, LifecycleMask::EXTENT),
    spec!(CloudTemporal, Compute, LifecycleMask::EXTENT),
    spec!(LensFlare, Compute, LifecycleMask::EXTENT),
    spec!(LensFlareTemporal, Compute, LifecycleMask::EXTENT),
    spec!(LensFlareSunVisible, Compute, LifecycleMask::EXTENT),
    spec!(Composition, Compute, LifecycleMask::EXTENT),
    spec!(PlayerCollider, Compute, LifecycleMask::EXTENT),
    spec!(TerrainQuery, Compute, LifecycleMask::EXTENT),
    spec!(WindVolume, Compute, LifecycleMask::EXTENT),
    spec!(PostProcessing, Compute, LifecycleMask::EXTENT),
    spec!(TerrainDepthPrefill, Graphics, LifecycleMask::EXTENT),
    spec!(
        Flora,
        Graphics,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_CONSUMER
    ),
    spec!(
        FloraLod,
        Graphics,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_CONSUMER
    ),
    spec!(
        Leaves,
        Graphics,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_CONSUMER
    ),
    spec!(
        LeavesLod,
        Graphics,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_CONSUMER
    ),
    spec!(LeavesShadowLod, Graphics, LifecycleMask::EXTENT),
    spec!(
        Sprinkler,
        Graphics,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_CONSUMER
    ),
    spec!(GeometryPreview, Graphics, LifecycleMask::EXTENT),
    spec!(
        EnvironmentProbeDepth,
        Graphics,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_CONSUMER
    ),
    spec!(
        EnvironmentProbeOverlay,
        Graphics,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_CONSUMER
    ),
    spec!(
        DynamicFruit,
        Graphics,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_CONSUMER
    ),
    spec!(DynamicFruitShadow, Graphics, 0),
    spec!(
        Particle,
        Graphics,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_CONSUMER
    ),
    spec!(
        WaterDroplet,
        Graphics,
        LifecycleMask::EXTENT | LifecycleMask::DDGI_CONSUMER
    ),
    spec!(Glass, Graphics, 0),
];

struct PreparedComputeGeneration {
    key: PipelineKey,
    descriptors: PreparedDescriptorGeneration,
}

struct PreparedGraphicsGeneration {
    key: PipelineKey,
    descriptors: PreparedDescriptorGeneration,
}

fn prepared_pair_matches(
    expected_key: PipelineKey,
    expected_kind: PipelineKind,
    prepared_key: PipelineKey,
    prepared_kind: PipelineKind,
) -> bool {
    (prepared_key, prepared_kind) == (expected_key, expected_kind)
}

/// A complete private descriptor generation for every pipeline that consumes a DDGI Volume.
/// Named fields make preparation/publication pairing independent of list position.
pub struct PreparedDdgiConsumerDescriptors {
    token_serial: u64,
    tracer: PreparedComputeGeneration,
    flora_lighting_cache: PreparedComputeGeneration,
    tree_leaf_lighting_cache: PreparedComputeGeneration,
    flora: PreparedGraphicsGeneration,
    flora_lod: PreparedGraphicsGeneration,
    leaves: PreparedGraphicsGeneration,
    leaves_lod: PreparedGraphicsGeneration,
    sprinkler: PreparedGraphicsGeneration,
    dynamic_fruit: PreparedGraphicsGeneration,
    particle: PreparedGraphicsGeneration,
    water_droplet: PreparedGraphicsGeneration,
    environment_probe_depth: PreparedGraphicsGeneration,
    environment_probe_overlay: PreparedGraphicsGeneration,
}

impl PreparedDdgiConsumerDescriptors {
    pub fn token_serial(&self) -> u64 {
        self.token_serial
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

    pub fn prepare_ddgi_consumers(&self, volume: &DdgiVolume) -> PreparedDdgiConsumerDescriptors {
        let irradiance_atlas = volume
            .published_irradiance_atlas()
            .unwrap_or(&volume.ddgi_irradiance_atlas);
        let visibility_atlas = volume
            .published_visibility_atlas()
            .unwrap_or(&volume.ddgi_visibility_atlas);
        let mut writes = vec![DescriptorWrite {
            name: "ddgi_probe_metadata",
            resource: DescriptorResource::Buffer(&volume.ddgi_probe_metadata),
        }];
        for (name, texture) in [
            (
                "ddgi_global_sky_irradiance",
                volume.published_global_sky_irradiance(),
            ),
            ("ddgi_irradiance_atlas", irradiance_atlas),
            ("ddgi_visibility_atlas", visibility_atlas),
        ] {
            writes.push(DescriptorWrite {
                name,
                resource: DescriptorResource::Texture(texture),
            });
        }
        let prepare_compute =
            |key: PipelineKey, pipeline: &ComputePipeline, error: &'static str| {
                PreparedComputeGeneration {
                    key,
                    descriptors: pipeline
                        .prepare_descriptors(DescriptorUpdate::Named(&writes))
                        .expect(error),
                }
            };
        let prepare_graphics =
            |key: PipelineKey, pipeline: &GraphicsPipeline, error: &'static str| {
                PreparedGraphicsGeneration {
                    key,
                    descriptors: pipeline
                        .prepare_descriptors(DescriptorUpdate::Named(&writes))
                        .expect(error),
                }
            };

        PreparedDdgiConsumerDescriptors {
            token_serial: volume
                .status()
                .build_token
                .expect("staged DDGI consumer descriptors require a build token")
                .serial(),
            tracer: prepare_compute(
                PipelineKey::Tracer,
                &self.compute.tracer_ppl,
                "DDGI consumer tracer descriptor preparation failed",
            ),
            flora_lighting_cache: prepare_compute(
                PipelineKey::FloraLightingCache,
                &self.compute.flora_lighting_cache_ppl,
                "DDGI consumer flora cache descriptor preparation failed",
            ),
            tree_leaf_lighting_cache: prepare_compute(
                PipelineKey::TreeLeafLightingCache,
                &self.compute.tree_leaf_lighting_cache_ppl,
                "DDGI consumer tree-leaf cache descriptor preparation failed",
            ),
            flora: prepare_graphics(
                PipelineKey::Flora,
                &self.graphics.flora_ppl,
                "DDGI consumer flora descriptor preparation failed",
            ),
            flora_lod: prepare_graphics(
                PipelineKey::FloraLod,
                &self.graphics.flora_lod_ppl,
                "DDGI consumer flora LOD descriptor preparation failed",
            ),
            leaves: prepare_graphics(
                PipelineKey::Leaves,
                &self.graphics.leaves_ppl,
                "DDGI consumer leaves descriptor preparation failed",
            ),
            leaves_lod: prepare_graphics(
                PipelineKey::LeavesLod,
                &self.graphics.leaves_lod_ppl,
                "DDGI consumer leaves LOD descriptor preparation failed",
            ),
            sprinkler: prepare_graphics(
                PipelineKey::Sprinkler,
                &self.graphics.sprinkler_ppl,
                "DDGI consumer sprinkler descriptor preparation failed",
            ),
            dynamic_fruit: prepare_graphics(
                PipelineKey::DynamicFruit,
                &self.graphics.dynamic_fruit_ppl,
                "DDGI consumer fruit descriptor preparation failed",
            ),
            particle: prepare_graphics(
                PipelineKey::Particle,
                &self.graphics.particle_ppl,
                "DDGI consumer particle descriptor preparation failed",
            ),
            water_droplet: prepare_graphics(
                PipelineKey::WaterDroplet,
                &self.graphics.water_droplet_ppl,
                "DDGI consumer droplet descriptor preparation failed",
            ),
            environment_probe_depth: prepare_graphics(
                PipelineKey::EnvironmentProbeDepth,
                &self.graphics.environment_probe_visualization_depth_ppl,
                "DDGI consumer probe-depth descriptor preparation failed",
            ),
            environment_probe_overlay: prepare_graphics(
                PipelineKey::EnvironmentProbeOverlay,
                &self.graphics.environment_probe_visualization_overlay_ppl,
                "DDGI consumer probe-overlay descriptor preparation failed",
            ),
        }
    }

    pub fn publish_ddgi_consumers(
        &self,
        expected_token_serial: u64,
        generation: u64,
        prepared: PreparedDdgiConsumerDescriptors,
    ) {
        assert_eq!(
            prepared.token_serial, expected_token_serial,
            "prepared DDGI consumer generation must match the promoted Volume"
        );
        let publish_compute = |expected_key: PipelineKey,
                               pipeline: &ComputePipeline,
                               prepared: PreparedComputeGeneration| {
            assert!(
                prepared_pair_matches(
                    expected_key,
                    PipelineKind::Compute,
                    prepared.key,
                    PipelineKind::Compute,
                ),
                "prepared compute generation must publish to its declared pipeline"
            );
            self.frame_retirement_sink
                .retire(pipeline.publish_prepared_descriptors(
                    "ddgi.consumer.descriptors",
                    generation,
                    prepared.descriptors,
                ));
        };
        let publish_graphics =
            |expected_key: PipelineKey,
             pipeline: &GraphicsPipeline,
             prepared: PreparedGraphicsGeneration| {
                assert!(
                    prepared_pair_matches(
                        expected_key,
                        PipelineKind::Graphics,
                        prepared.key,
                        PipelineKind::Graphics,
                    ),
                    "prepared graphics generation must publish to its declared pipeline"
                );
                self.frame_retirement_sink
                    .retire(pipeline.publish_prepared_descriptors(
                        "ddgi.consumer.descriptors",
                        generation,
                        prepared.descriptors,
                    ));
            };

        publish_compute(
            PipelineKey::Tracer,
            &self.compute.tracer_ppl,
            prepared.tracer,
        );
        publish_compute(
            PipelineKey::FloraLightingCache,
            &self.compute.flora_lighting_cache_ppl,
            prepared.flora_lighting_cache,
        );
        publish_compute(
            PipelineKey::TreeLeafLightingCache,
            &self.compute.tree_leaf_lighting_cache_ppl,
            prepared.tree_leaf_lighting_cache,
        );
        publish_graphics(PipelineKey::Flora, &self.graphics.flora_ppl, prepared.flora);
        publish_graphics(
            PipelineKey::FloraLod,
            &self.graphics.flora_lod_ppl,
            prepared.flora_lod,
        );
        publish_graphics(
            PipelineKey::Leaves,
            &self.graphics.leaves_ppl,
            prepared.leaves,
        );
        publish_graphics(
            PipelineKey::LeavesLod,
            &self.graphics.leaves_lod_ppl,
            prepared.leaves_lod,
        );
        publish_graphics(
            PipelineKey::Sprinkler,
            &self.graphics.sprinkler_ppl,
            prepared.sprinkler,
        );
        publish_graphics(
            PipelineKey::DynamicFruit,
            &self.graphics.dynamic_fruit_ppl,
            prepared.dynamic_fruit,
        );
        publish_graphics(
            PipelineKey::Particle,
            &self.graphics.particle_ppl,
            prepared.particle,
        );
        publish_graphics(
            PipelineKey::WaterDroplet,
            &self.graphics.water_droplet_ppl,
            prepared.water_droplet,
        );
        publish_graphics(
            PipelineKey::EnvironmentProbeDepth,
            &self.graphics.environment_probe_visualization_depth_ppl,
            prepared.environment_probe_depth,
        );
        publish_graphics(
            PipelineKey::EnvironmentProbeOverlay,
            &self.graphics.environment_probe_visualization_overlay_ppl,
            prepared.environment_probe_overlay,
        );
    }

    pub fn update_ddgi_consumers(
        &self,
        expected_token_serial: u64,
        volume: &DdgiVolume,
        generation: u64,
    ) {
        let prepared = self.prepare_ddgi_consumers(volume);
        self.publish_ddgi_consumers(expected_token_serial, generation, prepared);
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
        prepared_pair_matches, LifecycleMask, PipelineKey, PipelineKind, PIPELINE_LIFECYCLE,
    };
    use std::collections::HashSet;

    fn family_keys(lifecycle: u8) -> HashSet<PipelineKey> {
        PIPELINE_LIFECYCLE
            .iter()
            .filter(|spec| spec.lifecycle.contains(lifecycle))
            .map(|spec| spec.key)
            .collect()
    }

    #[test]
    fn pipeline_family_keys_are_unique() {
        let keys = PIPELINE_LIFECYCLE
            .iter()
            .map(|spec| spec.key)
            .collect::<HashSet<_>>();
        assert_eq!(keys.len(), PIPELINE_LIFECYCLE.len());
        assert_eq!(keys.len(), 51, "every concrete pipeline must have one key");
    }

    #[test]
    fn lifecycle_membership_is_complete() {
        assert!(PIPELINE_LIFECYCLE
            .iter()
            .all(|spec| spec.lifecycle.contains(LifecycleMask::CONSTRUCTION)));
        assert!(PIPELINE_LIFECYCLE
            .iter()
            .all(|spec| spec.lifecycle.contains(LifecycleMask::RECORD)));
        assert_eq!(
            PIPELINE_LIFECYCLE
                .iter()
                .filter(|spec| spec.kind == PipelineKind::Compute)
                .count(),
            36
        );
        assert_eq!(
            PIPELINE_LIFECYCLE
                .iter()
                .filter(|spec| spec.kind == PipelineKind::Graphics)
                .count(),
            15
        );
        assert_eq!(family_keys(LifecycleMask::EXTENT).len(), 45);

        let ddgi_builder = family_keys(LifecycleMask::DDGI_BUILDER);
        assert_eq!(ddgi_builder.len(), 9);
        assert_eq!(
            ddgi_builder,
            HashSet::from([
                PipelineKey::DdgiGlobalSkyFilter,
                PipelineKey::DdgiOctahedralGutter,
                PipelineKey::DdgiProbeRelocate,
                PipelineKey::DdgiProbeTrace,
                PipelineKey::DdgiIrradianceFilter,
                PipelineKey::DdgiVisibilityFilter,
                PipelineKey::DdgiIrradianceGutter,
                PipelineKey::DdgiVisibilityGutter,
                PipelineKey::DdgiAtlasReduce,
            ])
        );

        let ddgi_consumers = family_keys(LifecycleMask::DDGI_CONSUMER);
        assert_eq!(ddgi_consumers.len(), 13);
        assert_eq!(
            ddgi_consumers,
            HashSet::from([
                PipelineKey::Tracer,
                PipelineKey::FloraLightingCache,
                PipelineKey::TreeLeafLightingCache,
                PipelineKey::Flora,
                PipelineKey::FloraLod,
                PipelineKey::Leaves,
                PipelineKey::LeavesLod,
                PipelineKey::Sprinkler,
                PipelineKey::DynamicFruit,
                PipelineKey::Particle,
                PipelineKey::WaterDroplet,
                PipelineKey::EnvironmentProbeDepth,
                PipelineKey::EnvironmentProbeOverlay,
            ])
        );

        let glass = PIPELINE_LIFECYCLE
            .iter()
            .find(|spec| spec.key == PipelineKey::Glass)
            .expect("Glass must remain an opaque topology member");
        assert_eq!(
            glass.lifecycle,
            LifecycleMask::new(0),
            "Glass keeps construction/record behavior without resize or DDGI membership"
        );
    }

    #[test]
    fn prepared_generation_is_paired_with_actual_pipeline_type() {
        assert!(prepared_pair_matches(
            PipelineKey::Tracer,
            PipelineKind::Compute,
            PipelineKey::Tracer,
            PipelineKind::Compute,
        ));
        assert!(prepared_pair_matches(
            PipelineKey::Flora,
            PipelineKind::Graphics,
            PipelineKey::Flora,
            PipelineKind::Graphics,
        ));
        assert!(!prepared_pair_matches(
            PipelineKey::Flora,
            PipelineKind::Graphics,
            PipelineKey::Leaves,
            PipelineKind::Graphics,
        ));
        assert!(!prepared_pair_matches(
            PipelineKey::Tracer,
            PipelineKind::Compute,
            PipelineKey::Tracer,
            PipelineKind::Graphics,
        ));
    }
}
