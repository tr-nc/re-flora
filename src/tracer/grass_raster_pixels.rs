//! Bounded, streamed hardware-raster tiles. No per-plant draw calls or GPU readback.
//! The lattice/projection is shared with analytic stems; geometry is real triangles.
use super::TracerResources;
use crate::generated::gpu_structs::PushConstantGrassRasterPrepare;
use re_flora_vkn::{
    vk, Allocator, AttachmentDescOuter, AttachmentType, Buffer, BufferUsage, BufferUse,
    CommandBuffer, ComputePipeline, DescriptorPool, DescriptorResource, DescriptorUpdate, Extent2D,
    Extent3D, FrameRetirement, FrameRetirementSink, Framebuffer, GraphicsPipeline,
    GraphicsPipelineDesc, ImageDesc, MemoryLocation, PushConstantInfo, RenderPass, RenderTarget,
    SamplerDesc, ShaderModule, Texture, TextureLayout, Viewport, VulkanContext,
};
use std::sync::Arc;

const ATLAS_SIDE: u32 = 2048; // model_raster_grid.slang; bounded independently of population.
const VIEW_BYTES: u64 = 6 * 16; // ModelRasterView: six float4 records, GPU-only.
const MIN_SLOT: u32 = 32;

pub(super) fn slot_side(resolution: u32) -> u32 {
    (resolution.saturating_mul(5) / 4 + 4)
        .clamp(MIN_SLOT, 512)
        .next_power_of_two()
}
pub(super) fn capacity(side: u32) -> u32 {
    (ATLAS_SIDE / side).pow(2)
}

pub(super) struct GrassRasterPixels {
    atlas_color: Texture,
    atlas: RenderTarget,
    main_load: RenderTarget,
    main_key: (vk::Image, vk::Image),
    generation: u64,
    retirement: FrameRetirementSink,
    views: Arc<Buffer>,
    quad_indices: Buffer,
    prepare: ComputePipeline,
    clear: GraphicsPipeline,
    mesh: GraphicsPipeline,
    display: GraphicsPipeline,
    continuous: GraphicsPipeline,
}

fn target(
    ctx: &VulkanContext,
    color: &Texture,
    depth: &Texture,
    load: vk::AttachmentLoadOp,
) -> RenderTarget {
    let pass = RenderPass::with_attachments(
        ctx.device().clone(),
        &[
            AttachmentDescOuter {
                texture: color.clone(),
                load_op: load,
                store_op: vk::AttachmentStoreOp::STORE,
                initial_layout: TextureLayout::GENERAL,
                final_layout: TextureLayout::GENERAL,
                ty: AttachmentType::Color,
            },
            AttachmentDescOuter {
                texture: depth.clone(),
                load_op: load,
                store_op: vk::AttachmentStoreOp::STORE,
                initial_layout: TextureLayout::GENERAL,
                final_layout: TextureLayout::GENERAL,
                ty: AttachmentType::Depth,
            },
        ],
    );
    let framebuffer = Framebuffer::from_textures(
        ctx.clone(),
        &pass,
        &[color, depth],
        color.get_image().get_desc().extent.as_extent_2d().unwrap(),
    )
    .unwrap();
    RenderTarget::new(pass, vec![framebuffer])
}

impl GrassRasterPixels {
    pub fn new(
        ctx: &VulkanContext,
        allocator: Allocator,
        pool: &DescriptorPool,
        resources: &TracerResources,
        retirement: FrameRetirementSink,
    ) -> Self {
        let texture = |format, aspect, usage| {
            Texture::new(
                ctx.device().clone(),
                allocator.clone(),
                &ImageDesc {
                    extent: Extent3D::new(ATLAS_SIDE, ATLAS_SIDE, 1),
                    format,
                    aspect,
                    usage,
                    initial_layout: TextureLayout::UNDEFINED,
                    ..Default::default()
                },
                &SamplerDesc::default(),
            )
        };
        let atlas_color = texture(
            vk::Format::R32G32B32A32_SFLOAT,
            vk::ImageAspectFlags::COLOR,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        );
        let depth = texture(
            vk::Format::D32_SFLOAT,
            vk::ImageAspectFlags::DEPTH,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
        );
        let atlas = target(ctx, &atlas_color, &depth, vk::AttachmentLoadOp::DONT_CARE);
        let color = &resources.extent_dependent_resources.gfx_output_tex;
        let depth = &resources.extent_dependent_resources.gfx_depth_tex;
        let main_load = target(ctx, color, depth, vk::AttachmentLoadOp::LOAD);
        let shader = |name: &str| {
            ShaderModule::from_precompiled(ctx.device(), &format!("shader/foliage/{name}"), "main")
                .unwrap()
        };
        let prepare = ComputePipeline::new_uninitialized(
            ctx.device(),
            &shader("grass_raster_prepare.comp"),
            pool,
        );
        prepare
            .initialize_descriptors(DescriptorUpdate::SetContaining {
                anchor: "gui_input",
                providers: &[resources],
            })
            .unwrap();
        let graphics = |vert: &str, frag: &str, pass: &RenderPass, compare, blend| {
            let pipeline = GraphicsPipeline::new_uninitialized(
                ctx.device(),
                &shader(vert),
                &shader(frag),
                pass,
                &GraphicsPipelineDesc {
                    cull_mode: vk::CullModeFlags::NONE,
                    depth_test_enable: true,
                    depth_write_enable: true,
                    depth_compare_op: compare,
                    color_blend_enable: blend,
                    ..Default::default()
                },
                None,
                pool,
            );
            pipeline
                .initialize_descriptors(DescriptorUpdate::SetContaining {
                    anchor: "gui_input",
                    providers: &[resources],
                })
                .unwrap();
            pipeline
        };
        let clear = graphics(
            "grass_raster_clear.vert",
            "model_raster_clear.frag",
            atlas.get_render_pass(),
            vk::CompareOp::ALWAYS,
            false,
        );
        let mesh = graphics(
            "grass_raster_tile.vert",
            "model_raster_tile.frag",
            atlas.get_render_pass(),
            vk::CompareOp::LESS,
            false,
        );
        let display = graphics(
            "grass_raster_display.vert",
            "grass_raster_display.frag",
            main_load.get_render_pass(),
            vk::CompareOp::LESS,
            true,
        );
        let continuous = graphics(
            "grass_raster_fallback.vert",
            "stem_band.frag",
            main_load.get_render_pass(),
            vk::CompareOp::LESS,
            true,
        );
        let views = Arc::new(Buffer::new_sized(
            ctx.device().clone(),
            allocator.clone(),
            BufferUsage::from_flags(vk::BufferUsageFlags::STORAGE_BUFFER),
            MemoryLocation::GpuOnly,
            u64::from(capacity(MIN_SLOT)) * VIEW_BYTES,
        ));
        let quad_indices = Buffer::new_sized(
            ctx.device().clone(),
            allocator,
            BufferUsage::from_flags(vk::BufferUsageFlags::INDEX_BUFFER),
            MemoryLocation::CpuToGpu,
            6 * 4,
        );
        quad_indices.fill(&[0u32, 1, 2, 3, 4, 5]).unwrap();
        log::info!("[GRASS_RASTER_PIXELS] atlas={}x{} bytes={} max_batch={} grid=model depth=float32 draw=batched",
            ATLAS_SIDE, ATLAS_SIDE, ATLAS_SIDE * ATLAS_SIDE * 20, capacity(MIN_SLOT));
        Self {
            atlas_color,
            atlas,
            main_load,
            main_key: (color.get_image().as_raw(), depth.get_image().as_raw()),
            generation: 1,
            retirement,
            views,
            quad_indices,
            prepare,
            clear,
            mesh,
            display,
            continuous,
        }
    }

    pub fn begin_frame(&mut self, ctx: &VulkanContext, resources: &TracerResources, slot: usize) {
        let color = &resources.extent_dependent_resources.gfx_output_tex;
        let depth = &resources.extent_dependent_resources.gfx_depth_tex;
        let key = (color.get_image().as_raw(), depth.get_image().as_raw());
        if self.main_key != key {
            let new = target(
                ctx,
                color,
                &resources.extent_dependent_resources.gfx_depth_tex,
                vk::AttachmentLoadOp::LOAD,
            );
            let old = std::mem::replace(&mut self.main_load, new);
            self.retirement.retire(FrameRetirement::new(
                "stem_raster.main_load",
                self.generation,
                old,
            ));
            self.generation += 1;
            self.main_key = key;
        }
        self.prepare.begin_transient_descriptor_frame(slot);
        for pipeline in [&self.clear, &self.mesh, &self.display, &self.continuous] {
            pipeline.begin_transient_descriptor_frame(slot);
        }
    }

    /// Called outside a render pass. Each bounded batch prepares its descriptors,
    /// rasterizes private tiles, then loads and preserves the main color/depth.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        cmdbuf: &CommandBuffer,
        poses: &Buffer,
        vertices: &Buffer,
        indices: &Buffer,
        index_count: u32,
        pass: PushConstantGrassRasterPrepare,
    ) {
        let extent = Extent2D::new(pass.extent[0], pass.extent[1]);
        let viewport = Viewport::from_extent(extent);
        let scissor = vk::Rect2D {
            offset: vk::Offset2D::default(),
            extent: vk::Extent2D {
                width: extent.width,
                height: extent.height,
            },
        };
        let mut first = 0;
        while first < pass.instance_count {
            let count = (pass.instance_count - first).min(capacity(pass.slot_side));
            let push = PushConstantGrassRasterPrepare {
                first_instance: pass.first_instance + first,
                instance_count: count,
                ..pass
            };
            let push_info = PushConstantInfo {
                shader_stage: vk::ShaderStageFlags::VERTEX,
                push_constants: bytemuck::bytes_of(&push).to_vec(),
            };
            let display_push = PushConstantInfo {
                shader_stage: vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                push_constants: bytemuck::bytes_of(&push).to_vec(),
            };
            let resources = [
                ("stem_raster_views", DescriptorResource::Buffer(&self.views)),
                ("grass_band_pose_cache", DescriptorResource::Buffer(poses)),
            ];
            self.prepare
                .record_with_descriptors(
                    cmdbuf,
                    &resources,
                    Extent3D::new(count, 1, 1),
                    Some(bytemuck::bytes_of(&push)),
                )
                .unwrap();
            for pipeline in [&self.clear, &self.mesh, &self.display, &self.continuous] {
                pipeline.prepare_descriptor_resources(cmdbuf);
            }
            let clear = self
                .clear
                .prepare_draw_descriptors(cmdbuf, &resources[..1])
                .unwrap();
            let mesh = self
                .mesh
                .prepare_draw_descriptors(cmdbuf, &resources)
                .unwrap();
            // Initialize only the active cells. Full-atlas clears would turn
            // population growth into needless O(instances * max_tile_area) work.
            cmdbuf.use_buffer(&self.quad_indices, BufferUse::IndexRead);
            cmdbuf.use_buffer(vertices, BufferUse::VertexRead);
            cmdbuf.use_buffer(indices, BufferUse::IndexRead);
            self.atlas.record_begin(cmdbuf, &[]);
            let atlas_extent = Extent2D::new(ATLAS_SIDE, ATLAS_SIDE);
            let atlas_scissor = vk::Rect2D {
                offset: vk::Offset2D::default(),
                extent: vk::Extent2D {
                    width: ATLAS_SIDE,
                    height: ATLAS_SIDE,
                },
            };
            self.clear.record_viewport_scissor(
                cmdbuf,
                Viewport::from_extent(atlas_extent),
                atlas_scissor,
            );
            let instance = super::FloraInstanceResources::species_offset(push.species as usize)
                + push.first_instance;
            cmdbuf.bind_index_buffer_u32(&self.quad_indices);
            self.clear.record_indexed_with_prepared_descriptors(
                cmdbuf,
                &clear,
                6,
                count,
                0,
                0,
                instance,
                Some(&push_info),
            );
            cmdbuf.bind_index_buffer_u32(indices);
            cmdbuf.bind_vertex_buffers(0, &[vertices]);
            self.mesh.record_indexed_with_prepared_descriptors(
                cmdbuf,
                &mesh,
                index_count,
                count,
                0,
                0,
                instance,
                Some(&push_info),
            );
            self.atlas.record_end(cmdbuf);
            let display = self
                .display
                .prepare_draw_descriptors(
                    cmdbuf,
                    &[
                        ("stem_raster_views", DescriptorResource::Buffer(&self.views)),
                        (
                            "stem_raster_color",
                            DescriptorResource::Texture(&self.atlas_color),
                        ),
                    ],
                )
                .unwrap();
            let continuous = self
                .continuous
                .prepare_draw_descriptors(cmdbuf, &resources)
                .unwrap();
            self.main_load.record_begin(cmdbuf, &[]);
            self.display
                .record_viewport_scissor(cmdbuf, viewport, scissor);
            cmdbuf.bind_index_buffer_u32(&self.quad_indices);
            self.display.record_indexed_with_prepared_descriptors(
                cmdbuf,
                &display,
                6,
                count,
                0,
                0,
                instance,
                Some(&display_push),
            );
            cmdbuf.bind_index_buffer_u32(indices);
            cmdbuf.bind_vertex_buffers(0, &[vertices]);
            self.continuous.record_indexed_with_prepared_descriptors(
                cmdbuf,
                &continuous,
                index_count,
                count,
                0,
                0,
                instance,
                Some(&push_info),
            );
            self.main_load.record_end(cmdbuf);
            first += count;
        }
    }

    pub fn main_target(&self) -> &RenderTarget {
        &self.main_load
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slots_bound_storage_and_stream_every_instance() {
        for resolution in [1, 8, 32, 45, 64, 103, 192, 512, u32::MAX] {
            let side = slot_side(resolution);
            assert!(side.is_power_of_two() && (MIN_SLOT..=512).contains(&side));
            assert!(capacity(side) >= 16);
            let n = 82454u32;
            let batches = n.div_ceil(capacity(side));
            assert_eq!(
                (batches - 1) * capacity(side) + (n - (batches - 1) * capacity(side)),
                n
            );
        }
        assert_eq!(slot_side(45), 64);
        assert_eq!(std::mem::size_of::<PushConstantGrassRasterPrepare>(), 32);
    }
}
