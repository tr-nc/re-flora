mod resources;
// pub use resources::*;

mod blas;
pub use blas::*;

use ash::vk;

use crate::{
    util::ShaderCompiler,
    vkn::{allocator, Allocator, Device, VulkanContext},
};

pub struct AccelerationStructure {
    pub acc_device: ash::khr::acceleration_structure::Device,
}

impl AccelerationStructure {
    pub fn new(
        context: &VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
    ) -> Self {
        let acc_device =
            ash::khr::acceleration_structure::Device::new(&context.instance(), &context.device());

        let blas = Blas::new(context, allocator, &acc_device, shader_compiler);

        Self { acc_device }
    }
}
