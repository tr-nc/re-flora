mod resources;

mod utils;

mod blas;
use blas::*;

mod tlas;
use tlas::*;

use ash::vk;
use gpu_allocator::vulkan;

use crate::{
    util::ShaderCompiler,
    vkn::{allocator, Allocator, DescriptorPool, Device, VulkanContext},
};

pub struct AccelerationStructure {
    pub acc_device: ash::khr::acceleration_structure::Device,
}

impl AccelerationStructure {
    pub fn new(
        vulkan_ctx: &VulkanContext,
        allocator: Allocator,
        shader_compiler: &ShaderCompiler,
    ) -> Self {
        let acc_device = ash::khr::acceleration_structure::Device::new(
            &vulkan_ctx.instance(),
            &vulkan_ctx.device(),
        );

        let descriptor_pool = DescriptorPool::a_big_one(vulkan_ctx.device()).unwrap();

        let blas = Blas::new(
            vulkan_ctx,
            allocator,
            descriptor_pool,
            acc_device.clone(),
            shader_compiler,
        );

        Self { acc_device }
    }
}
