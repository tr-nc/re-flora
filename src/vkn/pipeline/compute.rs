use crate::vkn::{Device, PipelineLayout, ShaderModule};
use ash::vk::{self};
use std::ops::Deref;

pub struct ComputePipeline {
    device: Device,
    pipeline: vk::Pipeline,

    // saved for obtaining
    pipeline_layout: PipelineLayout,
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
    fn new(
        device: &Device,
        stage_info: &vk::PipelineShaderStageCreateInfo,
        pipeline_layout: PipelineLayout,
    ) -> Self {
        let create_info = vk::ComputePipelineCreateInfo::default()
            .stage(*stage_info)
            .layout(pipeline_layout.as_raw());
        let pipeline = Self::create_pipeline(device, create_info);
        Self {
            device: device.clone(),
            pipeline,
            pipeline_layout,
        }
    }

    pub fn from_shader_module(device: &Device, shader_module: ShaderModule) -> Self {
        let stage_info = shader_module.get_shader_stage_create_info();
        let pipeline_layout = shader_module.get_pipeline_layout(&device);
        Self::new(device, &stage_info, pipeline_layout)
    }

    pub fn get_pipeline_layout(&self) -> &PipelineLayout {
        &self.pipeline_layout
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
