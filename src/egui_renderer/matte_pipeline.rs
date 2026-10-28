use super::matte_blur::MatteBlur;
use re_flora_vkn::{
    vk, Allocator, CommandBuffer, DescriptorPool, DescriptorResource, DescriptorSet, Device,
    FormatOverride, FrameRetirement, FrameRetirementSink, GraphicsPipeline, GraphicsPipelineDesc,
    RenderPass, ShaderModule, Texture,
};

struct Backdrop {
    // Keep the source generation alive with its descriptor, including across resizes.
    texture: Texture,
    descriptor: DescriptorSet,
}

pub(super) struct MattePipeline {
    pub pipeline: GraphicsPipeline,
    backdrop: Option<Backdrop>,
    blur: MatteBlur,
    generation: u64,
    pixel_cell_size: u32,
    grid_origin: [i32; 2],
}

impl MattePipeline {
    pub fn new(
        device: &Device,
        allocator: Allocator,
        render_pass: &RenderPass,
        pool: &DescriptorPool,
    ) -> Self {
        let vertex =
            ShaderModule::from_precompiled(device, "shader/egui/ui_matte.vert", "main").unwrap();
        let fragment =
            ShaderModule::from_precompiled(device, "shader/egui/ui_matte.frag", "main").unwrap();
        let pipeline = Self::create_pipeline(device, render_pass, pool, &vertex, &fragment);
        Self {
            pipeline,
            backdrop: None,
            blur: MatteBlur::new(device, allocator, pool),
            generation: 0,
            pixel_cell_size: 0,
            grid_origin: [0; 2],
        }
    }

    fn create_pipeline(
        device: &Device,
        render_pass: &RenderPass,
        pool: &DescriptorPool,
        vertex: &ShaderModule,
        fragment: &ShaderModule,
    ) -> GraphicsPipeline {
        GraphicsPipeline::new_uninitialized(
            device,
            vertex,
            fragment,
            render_pass,
            &GraphicsPipelineDesc {
                format_overrides: vec![FormatOverride {
                    location: 2,
                    format: vk::Format::R8G8B8A8_UNORM,
                }],
                ..Default::default()
            },
            None,
            pool,
        )
    }

    pub fn prepare(
        &mut self,
        cmdbuf: &CommandBuffer,
        texture: &Texture,
        pixels_per_point: f32,
        pixel_cell_size: u32,
        bounds: egui::Rect,
        surface_origin: egui::Pos2,
        retirement: &FrameRetirementSink,
    ) {
        self.pixel_cell_size = pixel_cell_size;
        self.grid_origin = super::matte_glass::pixel_grid_origin(
            surface_origin,
            pixels_per_point,
            pixel_cell_size,
        );
        let texture = self.blur.prepare(
            cmdbuf,
            texture,
            pixels_per_point,
            pixel_cell_size,
            bounds,
            self.grid_origin,
            retirement,
        );
        let unchanged = self.backdrop.as_ref().is_some_and(|backdrop| {
            backdrop.texture.get_image().as_raw() == texture.get_image().as_raw()
        });
        if !unchanged {
            let descriptor = self
                .pipeline
                .allocate_transient_descriptor(
                    "backdrop_sampler",
                    DescriptorResource::Texture(texture),
                )
                .expect("matte backdrop descriptor");
            self.generation = self
                .generation
                .checked_add(1)
                .expect("matte generation overflow");
            if let Some(old) = self.backdrop.replace(Backdrop {
                texture: texture.clone(),
                descriptor,
            }) {
                retirement.retire(FrameRetirement::new(
                    "egui.matte.backdrop",
                    self.generation - 1,
                    old,
                ));
            }
        }
        self.pipeline
            .prepare_descriptor_set_resources(cmdbuf, &self.backdrop.as_ref().unwrap().descriptor);
    }

    pub fn bind(&self, cmdbuf: &CommandBuffer, projection: &[f32; 16]) {
        let push = crate::generated::gpu_structs::PushConstantUiMatte {
            ortho: bytemuck::cast(*projection),
            pixel_cell_size: self.pixel_cell_size,
            _pad0: [0; 4],
            grid_origin: self.grid_origin,
        };
        cmdbuf.push_vertex_constants(&self.pipeline, bytemuck::bytes_of(&push));
        let backdrop = self
            .backdrop
            .as_ref()
            .expect("matte surfaces require a prepared scene backdrop");
        self.pipeline
            .bind_descriptor_set(cmdbuf, "backdrop_sampler", &backdrop.descriptor)
            .expect("matte backdrop binding");
    }
}
