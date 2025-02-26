use crate::vkn::Device;
use ash::vk;
use std::ops::Deref;

pub struct GraphicsPipeline {
    device: ash::Device,
    pipeline: vk::Pipeline,
}

impl Drop for GraphicsPipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
        }
    }
}

impl Deref for GraphicsPipeline {
    type Target = vk::Pipeline;

    fn deref(&self) -> &Self::Target {
        &self.pipeline
    }
}

impl GraphicsPipeline {
    pub fn new(device: &Device, create_info: vk::GraphicsPipelineCreateInfo) -> Self {
        let pipeline = Self::create_pipeline(device, create_info);
        Self {
            device: device.as_raw().clone(),
            pipeline,
        }
    }

    pub fn as_raw(&self) -> vk::Pipeline {
        self.pipeline
    }

    fn create_pipeline(
        device: &ash::Device,
        create_info: vk::GraphicsPipelineCreateInfo,
    ) -> vk::Pipeline {
        unsafe {
            device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    std::slice::from_ref(&create_info),
                    None,
                )
                .map_err(|e| e.1)
                .unwrap()[0]
        }
    }
}
