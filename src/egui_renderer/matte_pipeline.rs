use re_flora_vkn::{
    vk, CommandBuffer, DescriptorPool, DescriptorResource, DescriptorSet, Device, FormatOverride,
    FrameRetirement, FrameRetirementSink, GraphicsPipeline, GraphicsPipelineDesc, RenderPass,
    ShaderModule, Texture,
};

struct Backdrop {
    // Keep the source generation alive with its descriptor, including across resizes.
    texture: Texture,
    descriptor: DescriptorSet,
}

pub(super) struct MattePipeline {
    pub pipeline: GraphicsPipeline,
    vertex: ShaderModule,
    fragment: ShaderModule,
    backdrop: Option<Backdrop>,
    generation: u64,
}

impl MattePipeline {
    pub fn new(device: &Device, render_pass: &RenderPass, pool: &DescriptorPool) -> Self {
        let vertex =
            ShaderModule::from_precompiled(device, "shader/egui/ui_matte.vert", "main").unwrap();
        let fragment =
            ShaderModule::from_precompiled(device, "shader/egui/ui_matte.frag", "main").unwrap();
        let pipeline = Self::create_pipeline(device, render_pass, pool, &vertex, &fragment);
        Self {
            pipeline,
            vertex,
            fragment,
            backdrop: None,
            generation: 0,
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

    pub fn set_render_pass(
        &mut self,
        device: &Device,
        render_pass: &RenderPass,
        pool: &DescriptorPool,
        retirement: &FrameRetirementSink,
    ) {
        let pipeline =
            Self::create_pipeline(device, render_pass, pool, &self.vertex, &self.fragment);
        let old = std::mem::replace(&mut self.pipeline, pipeline);
        retirement.retire(FrameRetirement::new(
            "egui.matte.pipeline",
            self.generation,
            old,
        ));
        if let Some(backdrop) = self.backdrop.take() {
            retirement.retire(FrameRetirement::new(
                "egui.matte.backdrop",
                self.generation,
                backdrop,
            ));
        }
    }

    pub fn prepare(
        &mut self,
        cmdbuf: &CommandBuffer,
        texture: &Texture,
        retirement: &FrameRetirementSink,
    ) {
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

    pub fn bind(&self, cmdbuf: &CommandBuffer) {
        let backdrop = self
            .backdrop
            .as_ref()
            .expect("matte surfaces require a prepared scene backdrop");
        self.pipeline
            .bind_descriptor_set(cmdbuf, "backdrop_sampler", &backdrop.descriptor)
            .expect("matte backdrop binding");
    }
}
