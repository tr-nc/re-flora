//! Cloud implementation and GPU lifecycle. The tracer schedules two effect phases;
//! it never copies, clears, or interprets cloud textures/history itself.

use crate::resource::Resource;
use re_flora_vkn::{
    vk, Allocator, ClearValue, ColorClearValue, CommandBuffer, ComputePipeline, DescriptorPool,
    Device, Extent2D, ImageDesc, ResourceContainer, SamplerDesc, ShaderModule, Texture,
    TextureLayout,
};
use resource_container_derive::ResourceContainer;

const SHADOW_MAP_RESOLUTION: u32 = 256;

#[derive(ResourceContainer)]
pub struct CloudScreenResources {
    cloud_raw_tex: Resource<Texture>,
    cloud_history_tex: Resource<Texture>,
    cloud_output_tex: Resource<Texture>,
}

impl CloudScreenResources {
    pub fn new(device: Device, allocator: Allocator, extent: Extent2D) -> Self {
        // Match the internal render grid, without an additional downsample.
        let texture = || {
            cloud_texture(
                device.clone(),
                allocator.clone(),
                extent,
                vk::Format::R16G16B16A16_SFLOAT,
            )
        };
        Self {
            cloud_raw_tex: Resource::new(texture()),
            cloud_history_tex: Resource::new(texture()),
            cloud_output_tex: Resource::new(texture()),
        }
    }
}

#[derive(ResourceContainer)]
pub struct CloudShadowResources {
    cloud_shadow_raw_tex: Resource<Texture>,
    cloud_shadow_history_tex: Resource<Texture>,
    cloud_shadow_tex: Resource<Texture>,
}

impl CloudShadowResources {
    pub fn new(device: Device, allocator: Allocator) -> Self {
        let extent = Extent2D::new(SHADOW_MAP_RESOLUTION, SHADOW_MAP_RESOLUTION);
        log::info!(
            "[CLOUD_SHADOW] using Beer transmittance map {}x{} with temporal resolve",
            extent.width,
            extent.height,
        );
        let texture = || {
            cloud_texture(
                device.clone(),
                allocator.clone(),
                extent,
                vk::Format::R16_SFLOAT,
            )
        };
        Self {
            cloud_shadow_raw_tex: Resource::new(texture()),
            cloud_shadow_history_tex: Resource::new(texture()),
            cloud_shadow_tex: Resource::new(texture()),
        }
    }

    pub fn clear(&self, cmdbuf: &CommandBuffer) {
        for texture in [&self.cloud_shadow_raw_tex, &self.cloud_shadow_tex] {
            clear(texture, cmdbuf, [1.0, 0.0, 0.0, 0.0]);
        }
    }
}

fn cloud_texture(
    device: Device,
    allocator: Allocator,
    extent: Extent2D,
    format: vk::Format,
) -> Texture {
    let image = ImageDesc {
        extent: extent.into(),
        format,
        usage: vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST,
        initial_layout: TextureLayout::UNDEFINED,
        aspect: vk::ImageAspectFlags::COLOR,
        ..Default::default()
    };
    let sampler = SamplerDesc {
        mag_filter: vk::Filter::LINEAR,
        min_filter: vk::Filter::LINEAR,
        ..Default::default()
    };
    Texture::new(device, allocator, &image, &sampler)
}

fn clear(texture: &Texture, cmdbuf: &CommandBuffer, value: [f32; 4]) {
    texture.get_image().record_clear(
        cmdbuf,
        Some(TextureLayout::GENERAL),
        0,
        ClearValue::Color(ColorClearValue::Float(value)),
    );
}

fn store_history(cmdbuf: &CommandBuffer, current: &Texture, previous: &Texture) {
    current.get_image().record_copy_to(
        cmdbuf,
        previous.get_image(),
        TextureLayout::GENERAL,
        TextureLayout::GENERAL,
    );
}

#[derive(Default)]
pub(super) struct CloudRuntime {
    screen_valid: bool,
    shadow_valid: bool,
}

impl CloudRuntime {
    pub(super) fn invalidate(&mut self) {
        self.invalidate_screen();
        self.invalidate_shadow();
    }

    pub(super) fn invalidate_screen(&mut self) {
        self.screen_valid = false;
    }

    pub(super) fn invalidate_shadow(&mut self) {
        self.shadow_valid = false;
    }

    pub(super) fn shadow_ready(&self) -> bool {
        self.shadow_valid
    }
}

pub(super) struct CloudShaders {
    screen: ShaderModule,
    screen_temporal: ShaderModule,
    shadow: ShaderModule,
    shadow_temporal: ShaderModule,
}

impl CloudShaders {
    pub(super) fn new(device: &Device) -> Self {
        let load = |path| ShaderModule::from_precompiled(device, path, "main").unwrap();
        Self {
            screen: load("shader/tracer/cloud.comp"),
            screen_temporal: load("shader/tracer/cloud_temporal.comp"),
            shadow: load("shader/tracer/cloud_shadow.comp"),
            shadow_temporal: load("shader/tracer/cloud_shadow_temporal.comp"),
        }
    }
}

pub(super) struct CloudPasses {
    screen: ComputePipeline,
    screen_temporal: ComputePipeline,
    shadow: ComputePipeline,
    shadow_temporal: ComputePipeline,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TemporalPushConstants {
    reset_history: u32,
}

impl CloudPasses {
    pub(super) fn new(
        device: &Device,
        shaders: &CloudShaders,
        pool: &DescriptorPool,
        resources: &dyn ResourceContainer,
    ) -> Self {
        let pipeline = |shader| ComputePipeline::new(device, shader, pool, &[resources]);
        Self {
            screen: pipeline(&shaders.screen),
            screen_temporal: pipeline(&shaders.screen_temporal),
            shadow: pipeline(&shaders.shadow),
            shadow_temporal: pipeline(&shaders.shadow_temporal),
        }
    }

    // The topology's extent transaction owns descriptor retirement. Register the
    // effect's pipelines as a group; their individual resources stay private.
    pub(super) fn pipelines(&self) -> [&ComputePipeline; 4] {
        [
            &self.screen,
            &self.screen_temporal,
            &self.shadow,
            &self.shadow_temporal,
        ]
    }
}

impl CloudRuntime {
    pub(super) fn record_shadow(
        &mut self,
        cmdbuf: &CommandBuffer,
        passes: &CloudPasses,
        resources: &CloudShadowResources,
    ) {
        let extent = resources.cloud_shadow_raw_tex.get_image().get_desc().extent;
        passes.shadow.record(cmdbuf, extent, None);
        let push = TemporalPushConstants {
            reset_history: u32::from(!self.shadow_valid),
        };
        passes
            .shadow_temporal
            .record(cmdbuf, extent, Some(bytemuck::bytes_of(&push)));
        store_history(
            cmdbuf,
            &resources.cloud_shadow_tex,
            &resources.cloud_shadow_history_tex,
        );
        self.shadow_valid = true;
    }

    pub(super) fn record_screen(
        &mut self,
        cmdbuf: &CommandBuffer,
        passes: &CloudPasses,
        resources: &CloudScreenResources,
        enabled: bool,
    ) {
        if !enabled {
            self.invalidate();
            for texture in [&resources.cloud_raw_tex, &resources.cloud_output_tex] {
                clear(texture, cmdbuf, [0.0; 4]);
            }
            return;
        }
        let extent = resources.cloud_raw_tex.get_image().get_desc().extent;
        passes.screen.record(cmdbuf, extent, None);
        let push = TemporalPushConstants {
            reset_history: u32::from(!self.screen_valid),
        };
        passes
            .screen_temporal
            .record(cmdbuf, extent, Some(bytemuck::bytes_of(&push)));
        store_history(
            cmdbuf,
            &resources.cloud_output_tex,
            &resources.cloud_history_tex,
        );
        self.screen_valid = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histories_have_independent_camera_and_light_space_invalidation() {
        let mut history = CloudRuntime {
            screen_valid: true,
            shadow_valid: true,
        };
        history.invalidate_screen();
        assert!(!history.screen_valid);
        assert!(history.shadow_ready());
        history.screen_valid = true;
        history.invalidate_shadow();
        assert!(history.screen_valid);
        assert!(!history.shadow_ready());
        history.shadow_valid = true;
        history.invalidate();
        assert!(!history.screen_valid);
        assert!(!history.shadow_ready());
    }
}
