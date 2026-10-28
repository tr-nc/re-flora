//! Painter-order UI composition. Every glass surface starts a new backdrop read.
use super::matte_glass;
use egui::{epaint::Primitive, ClippedPrimitive};
use re_flora_vkn::{
    vk, Allocator, AttachmentDesc, AttachmentReference, DescriptorPool, FormatOverride,
    FrameRetirement, FrameRetirementSink, Framebuffer, GraphicsPipeline, GraphicsPipelineDesc,
    ImageDesc, RenderPass, RenderPassDesc, RenderTarget, ShaderModule, SubpassDesc, Texture,
    TextureLayout, VulkanContext,
};
use std::ops::Range;

#[cfg(test)]
mod tests;

#[derive(Debug, PartialEq)]
pub(super) struct DrawPass {
    pub primitives: Range<usize>,
    pub index_offset: u32,
    pub vertex_offset: i32,
    pub blur_before: bool,
}

impl DrawPass {
    /// Only the actual clipped glass meshes consume the backdrop. Own text and
    /// later ordinary UI must not enlarge this surface's filter footprint.
    pub fn backdrop_bounds(&self, primitives: &[ClippedPrimitive]) -> egui::Rect {
        primitives[self.primitives.clone()]
            .iter()
            .fold(egui::Rect::NOTHING, |bounds, p| {
                if let Primitive::Mesh(mesh) = &p.primitive {
                    if mesh.texture_id == matte_glass::TEXTURE_ID {
                        let visible = mesh.calc_bounds().intersect(p.clip_rect);
                        if visible.is_positive() {
                            return bounds.union(visible);
                        }
                    }
                }
                bounds
            })
    }
}

/// Callbacks are explicit surface boundaries: equal-texture adjacent meshes must
/// not be merged into one backdrop evaluation by egui's tessellator.
pub(super) fn draw_passes(primitives: &[ClippedPrimitive]) -> Vec<DrawPass> {
    let mut result = Vec::new();
    let mut current = DrawPass {
        primitives: 0..0,
        index_offset: 0,
        vertex_offset: 0,
        blur_before: false,
    };
    let (mut index_offset, mut vertex_offset) = (0, 0);
    for (i, primitive) in primitives.iter().enumerate() {
        if matte_glass::is_backdrop_boundary(&primitive.primitive) {
            if current.primitives.start < i {
                current.primitives.end = i;
                result.push(current);
            }
            current = DrawPass {
                primitives: i + 1..i + 1,
                index_offset,
                vertex_offset,
                blur_before: true,
            };
        } else if let Primitive::Mesh(mesh) = &primitive.primitive {
            index_offset += mesh.indices.len() as u32;
            vertex_offset += mesh.vertices.len() as i32;
        }
    }
    if current.primitives.start < primitives.len() {
        current.primitives.end = primitives.len();
        result.push(current);
    }
    result
}

pub(super) struct CompositeTarget {
    pub image: Texture,
    pub render_target: RenderTarget,
}

pub(super) struct UiCompositor {
    context: VulkanContext,
    allocator: Allocator,
    pub render_pass: RenderPass,
    pub pipeline: GraphicsPipeline,
    target: Option<CompositeTarget>,
    generation: u64,
}

impl UiCompositor {
    pub fn new(
        context: VulkanContext,
        allocator: Allocator,
        pool: &DescriptorPool,
        vertex: &ShaderModule,
        fragment: &ShaderModule,
    ) -> Self {
        let render_pass = RenderPass::from_desc(
            context.device().clone(),
            RenderPassDesc {
                attachments: vec![AttachmentDesc {
                    format: vk::Format::R16G16B16A16_SFLOAT,
                    samples: vk::SampleCountFlags::TYPE_1,
                    load_op: vk::AttachmentLoadOp::LOAD,
                    store_op: vk::AttachmentStoreOp::STORE,
                    stencil_load_op: vk::AttachmentLoadOp::DONT_CARE,
                    stencil_store_op: vk::AttachmentStoreOp::DONT_CARE,
                    initial_layout: TextureLayout::COLOR_ATTACHMENT,
                    final_layout: TextureLayout::COLOR_ATTACHMENT,
                }],
                subpasses: vec![SubpassDesc {
                    color_attachments: vec![AttachmentReference {
                        attachment: 0,
                        layout: TextureLayout::COLOR_ATTACHMENT,
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        let pipeline = GraphicsPipeline::new_uninitialized(
            context.device(),
            vertex,
            fragment,
            &render_pass,
            &GraphicsPipelineDesc {
                format_overrides: vec![FormatOverride {
                    location: 2,
                    format: vk::Format::R8G8B8A8_UNORM,
                }],
                ..Default::default()
            },
            None,
            pool,
        );
        Self {
            context,
            allocator,
            render_pass,
            pipeline,
            target: None,
            generation: 0,
        }
    }

    pub fn prepare(
        &mut self,
        cmd: &re_flora_vkn::CommandBuffer,
        scene: &Texture,
        retirement: &FrameRetirementSink,
    ) {
        let extent = scene.get_image().get_desc().extent;
        if !self
            .target
            .as_ref()
            .is_some_and(|target| target.image.get_image().get_desc().extent == extent)
        {
            let image = Texture::new(
                self.context.device().clone(),
                self.allocator.clone(),
                &ImageDesc {
                    extent,
                    image_type_override: Some(vk::ImageType::TYPE_2D),
                    format: vk::Format::R16G16B16A16_SFLOAT,
                    usage: vk::ImageUsageFlags::COLOR_ATTACHMENT
                        | vk::ImageUsageFlags::STORAGE
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | vk::ImageUsageFlags::TRANSFER_DST,
                    aspect: vk::ImageAspectFlags::COLOR,
                    ..Default::default()
                },
                &Default::default(),
            );
            let framebuffer = Framebuffer::from_textures(
                self.context.clone(),
                &self.render_pass,
                &[&image],
                extent.as_extent_2d().expect("2D UI composite"),
            )
            .expect("UI composite framebuffer");
            let target = CompositeTarget {
                image,
                render_target: RenderTarget::new(self.render_pass.clone(), vec![framebuffer]),
            };
            if let Some(old) = self.target.replace(target) {
                retirement.retire(FrameRetirement::new("egui.composite", self.generation, old));
            }
            self.generation = self
                .generation
                .checked_add(1)
                .expect("UI composite generation overflow");
        }
        // Start from the current scene every frame. Never feed last frame's UI
        // back into the scene or sample an attachment inside its write pass.
        scene
            .get_image()
            .record_blit_to(cmd, self.target().image.get_image(), vk::Filter::NEAREST);
    }

    pub fn target(&self) -> &CompositeTarget {
        self.target.as_ref().expect("prepared UI composite target")
    }
}
