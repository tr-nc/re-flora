use crate::vkn::{CommandBuffer, DescriptorSet, Device, PipelineLayout, ShaderModule};
use ash::vk::{self};
use std::ops::Deref;

pub struct ComputePipeline {
    device: Device,
    pipeline: vk::Pipeline,
    pipeline_layout: PipelineLayout,
    // shader_module: ShaderModule,
    workgroup_size: [u32; 3],
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
        shader_module: &ShaderModule,
    ) -> Self {
        let create_info = vk::ComputePipelineCreateInfo::default()
            .stage(*stage_info)
            .layout(pipeline_layout.as_raw());

        let pipeline = unsafe {
            device
                .create_compute_pipelines(
                    vk::PipelineCache::null(),
                    std::slice::from_ref(&create_info),
                    None,
                )
                .map_err(|e| e.1)
                .unwrap()[0]
        };

        let workgroup_size = shader_module
            .get_workgroup_size()
            .expect("Failed to get workgroup size");

        Self {
            device: device.clone(),
            pipeline,
            pipeline_layout,
            workgroup_size,
        }
    }

    pub fn from_shader_module(device: &Device, shader_module: &ShaderModule) -> Self {
        let stage_info = shader_module.get_shader_stage_create_info();
        let pipeline_layout = PipelineLayout::from_shader_module(device, shader_module);
        Self::new(device, &stage_info, pipeline_layout, shader_module)
    }

    pub fn get_layout(&self) -> &PipelineLayout {
        &self.pipeline_layout
    }

    pub fn record_bind_descriptor_sets(
        &self,
        command_buffer: &CommandBuffer,
        descriptor_sets: &[DescriptorSet],
        first_set: u32,
    ) {
        let descriptor_sets = descriptor_sets
            .iter()
            .map(|s| s.as_raw())
            .collect::<Vec<_>>();

        unsafe {
            self.device.cmd_bind_descriptor_sets(
                command_buffer.as_raw(),
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout.as_raw(),
                first_set,
                &descriptor_sets,
                &[],
            );
        }
    }

    pub fn record_bind(&self, command_buffer: &CommandBuffer) {
        unsafe {
            self.device.cmd_bind_pipeline(
                command_buffer.as_raw(),
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline,
            );
        }
    }

    pub fn record_dispatch(&self, command_buffer: &CommandBuffer, dispatch_size: [u32; 3]) {
        let x = (dispatch_size[0] as f32 / self.workgroup_size[0] as f32).ceil() as u32;
        let y = (dispatch_size[1] as f32 / self.workgroup_size[1] as f32).ceil() as u32;
        let z = (dispatch_size[2] as f32 / self.workgroup_size[2] as f32).ceil() as u32;
        unsafe {
            self.device.cmd_dispatch(command_buffer.as_raw(), x, y, z);
        }
    }
}
