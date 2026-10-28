//! Current-composite Gaussian frost or compact cell means, refreshed per surface.
use re_flora_vkn::{
    vk, Allocator, CommandBuffer, ComputePipeline, DescriptorPool, DescriptorResource,
    DescriptorUpdate, Device, FrameRetirement, FrameRetirementSink, ImageDesc, ResourceContainer,
    ResourceLookup, SamplerDesc, ShaderModule, Texture,
};

mod region;
use region::FilterPlan;

struct PassResources<'a> {
    source: &'a Texture,
    output: &'a Texture,
}

impl ResourceContainer for PassResources<'_> {
    fn resolve_resource(&self, name: &str) -> ResourceLookup<'_> {
        match name {
            "blur_source" => ResourceLookup::Unique(DescriptorResource::Texture(self.source)),
            "blur_output" => ResourceLookup::Unique(DescriptorResource::Texture(self.output)),
            _ => ResourceLookup::Missing,
        }
    }
}

struct Images {
    source: Texture,
    horizontal: Texture,
    blurred: Texture,
}

pub(super) struct MatteBlur {
    device: Device,
    allocator: Allocator,
    passes: [ComputePipeline; 2],
    images: Option<Images>,
    generation: u64,
}

impl MatteBlur {
    pub fn new(device: &Device, allocator: Allocator, pool: &DescriptorPool) -> Self {
        let shader =
            ShaderModule::from_precompiled(device, "shader/egui/ui_matte_blur.comp", "main")
                .expect("matte blur shader");
        Self {
            device: device.clone(),
            allocator,
            passes: std::array::from_fn(|_| {
                ComputePipeline::new_uninitialized(device, &shader, pool)
            }),
            images: None,
            generation: 0,
        }
    }

    pub fn prepare(
        &mut self,
        cmd: &CommandBuffer,
        source: &Texture,
        pixels_per_point: f32,
        pixel_cell_size: u32,
        bounds: egui::Rect,
        grid_origin: [i32; 2],
        retirement: &FrameRetirementSink,
    ) -> &Texture {
        let unchanged = self.images.as_ref().is_some_and(|images| {
            images.source.get_image().as_raw() == source.get_image().as_raw()
        });
        if !unchanged {
            let desc = ImageDesc {
                extent: source.get_image().get_desc().extent,
                image_type_override: Some(vk::ImageType::TYPE_2D),
                format: vk::Format::R16G16B16A16_SFLOAT,
                usage: vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
                aspect: vk::ImageAspectFlags::COLOR,
                ..Default::default()
            };
            let sampler = SamplerDesc {
                min_filter: vk::Filter::LINEAR,
                mag_filter: vk::Filter::LINEAR,
                ..Default::default()
            };
            let images = Images {
                source: source.clone(),
                horizontal: Texture::new(
                    self.device.clone(),
                    self.allocator.clone(),
                    &desc,
                    &sampler,
                ),
                blurred: Texture::new(self.device.clone(), self.allocator.clone(), &desc, &sampler),
            };
            for (pipeline, resources) in self.passes.iter().zip([
                PassResources {
                    source: &images.source,
                    output: &images.horizontal,
                },
                PassResources {
                    source: &images.horizontal,
                    output: &images.blurred,
                },
            ]) {
                let providers: [&dyn ResourceContainer; 1] = [&resources];
                let update = DescriptorUpdate::All(&providers);
                if self.images.is_none() {
                    pipeline
                        .initialize_descriptors(update)
                        .expect("matte blur descriptors");
                } else {
                    retirement.retire(
                        pipeline
                            .publish_descriptors(
                                "egui.matte.blur.descriptors",
                                self.generation,
                                update,
                            )
                            .expect("matte blur descriptor generation"),
                    );
                }
            }
            if let Some(old) = self.images.replace(images) {
                retirement.retire(FrameRetirement::new(
                    "egui.matte.blur.images",
                    self.generation,
                    old,
                ));
            }
            self.generation = self
                .generation
                .checked_add(1)
                .expect("matte blur generation overflow");
        }
        let images = self.images.as_ref().unwrap();
        let viewport = source.get_image().get_desc().extent;
        let plan = FilterPlan::new(
            bounds,
            [viewport.width, viewport.height],
            pixels_per_point,
            pixel_cell_size,
            grid_origin,
        );
        for (index, pipeline) in self.passes.iter().enumerate() {
            let region = plan.passes[index];
            if region.extent.contains(&0) {
                continue;
            }
            let push = crate::generated::gpu_structs::PushConstantUiMatteBlur {
                sigma: plan.sigma,
                horizontal: u32::from(index == 0),
                pixel_cell_size,
                radius: plan.radius,
                dispatch_origin: region.origin,
                dispatch_extent: region.extent,
                grid_origin,
                _pad0: [0; 8],
            };
            // Dependencies still cover composite -> H -> V -> fragment and all
            // surfaces/frames. Only the spatial workload changes.
            let mut extent = viewport;
            extent.width = region.extent[0];
            extent.height = region.extent[1];
            pipeline.record(cmd, extent, Some(bytemuck::bytes_of(&push)));
        }
        &images.blurred
    }
}
