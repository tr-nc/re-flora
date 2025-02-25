use ash::vk;

use crate::shader_util::ShaderModule;

pub struct ComputePipeline {
    shader: ShaderModule,
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
}

impl ComputePipeline {
    pub fn new(loaded_shader: ShaderModule) -> Self {
        Self {}
    }
}
