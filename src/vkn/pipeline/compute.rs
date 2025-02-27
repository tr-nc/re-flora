use crate::vkn::{Device, ShaderModule};
use ash::vk;
use std::ops::Deref;

pub struct ComputePipeline {
    device: Device,
    pipeline: vk::Pipeline,
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
        }
    }
}

impl Deref for ComputePipeline {
    type Target = vk::Pipeline;

    fn deref(&self) -> &Self::Target {
        &self.pipeline
    }
}

impl ComputePipeline {
    fn new(device: &Device, create_info: vk::ComputePipelineCreateInfo) -> Self {
        let pipeline = Self::create_pipeline(device, create_info);
        Self {
            device: device.clone(),
            pipeline,
        }
    }

    pub fn from_shader_module(device: &Device, shader_module: ShaderModule) -> Self {
        let stage_info = shader_module.get_shader_stage_create_info();
        let compute_pipeline_layout = shader_module.get_shader_pipeline_layout(&device);
        let create_info = vk::ComputePipelineCreateInfo::default()
            .stage(stage_info)
            .layout(compute_pipeline_layout.as_raw());
        Self::new(device, create_info)
    }

    pub fn as_raw(&self) -> vk::Pipeline {
        self.pipeline
    }

    fn create_pipeline(
        device: &Device,
        create_info: vk::ComputePipelineCreateInfo,
    ) -> vk::Pipeline {
        unsafe {
            device
                .create_compute_pipelines(
                    vk::PipelineCache::null(),
                    std::slice::from_ref(&create_info),
                    None,
                )
                .map_err(|e| e.1)
                .unwrap()[0]
        }
    }
}
