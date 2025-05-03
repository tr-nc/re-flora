mod resources;

mod utils;

mod blas;
use blas::*;

mod tlas;
use tlas::*;

use crate::{
    util::ShaderCompiler,
    vkn::{Allocator, DescriptorPool, VulkanContext},
};

pub struct AccelerationStructure {
    pub acc_device: ash::khr::acceleration_structure::Device,
    pub blas: Blas,
    pub tlas: Tlas,
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
            allocator.clone(),
            descriptor_pool,
            acc_device.clone(),
            shader_compiler,
        );

        let tlas = Tlas::new(
            vulkan_ctx,
            allocator.clone(),
            acc_device.clone(),
            blas.as_raw(),
        );

        Self {
            acc_device,
            blas,
            tlas,
        }
    }

    pub fn blas(&self) -> &Blas {
        &self.blas
    }

    pub fn tlas(&self) -> &Tlas {
        &self.tlas
    }
}
